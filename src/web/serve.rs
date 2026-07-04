//! HTTP server with optional TLS + HTTP/2 (ALPN h2).

use std::future::Future;
use std::pin::Pin;

use axum::Router;
use axum::body::Body;
use hyper::body::Incoming;
use hyper_util::rt::{TokioExecutor, TokioIo};
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio_rustls::TlsAcceptor;
use tower_service::Service;
use tracing::{error, warn};

pub async fn serve_tls(
    listener: TcpListener,
    app: Router,
    tls: TlsAcceptor,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    let mut shutdown = Pin::from(Box::pin(shutdown));
    loop {
        let (stream, addr) = tokio::select! {
            () = &mut shutdown => break,
            accept = listener.accept() => accept?,
        };

        let tls = tls.clone();
        let tower_service = app.clone();
        tokio::spawn(async move {
            let Ok(stream) = tls.accept(stream).await else {
                error!("TLS handshake failed from {addr}");
                return;
            };
            let stream = TokioIo::new(stream);
            let hyper_service = hyper::service::service_fn(move |request: hyper::Request<Incoming>| {
                tower_service.clone().call(request.map(Body::new))
            });
            if let Err(err) = hyper_util::server::conn::auto::Builder::new(TokioExecutor::new())
                .serve_connection_with_upgrades(stream, hyper_service)
                .await
            {
                warn!("error serving connection from {addr}: {err}");
            }
        });
    }
    Ok(())
}

/// Plain HTTP + TLS (HTTP/2) on two listeners sharing one router and shutdown signal.
pub async fn serve_http_and_tls(
    http_listener: TcpListener,
    tls_listener: TcpListener,
    app: Router,
    tls: TlsAcceptor,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    let (shutdown_tx, _) = broadcast::channel::<()>(1);
    let mut plain_shutdown = shutdown_tx.subscribe();
    let mut tls_shutdown = shutdown_tx.subscribe();
    tokio::spawn(async move {
        shutdown.await;
        let _ = shutdown_tx.send(());
    });

    let app_plain = app.clone();
    let plain = tokio::spawn(async move {
        axum::serve(http_listener, app_plain)
            .with_graceful_shutdown(async move {
                let _ = plain_shutdown.recv().await;
            })
            .await
    });

    let tls_result = serve_tls(tls_listener, app, tls, async move {
        let _ = tls_shutdown.recv().await;
    })
    .await;

    let _ = plain.await;
    tls_result
}
