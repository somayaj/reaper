use std::net::{IpAddr, SocketAddr, TcpListener};
use std::time::{SystemTime, UNIX_EPOCH};

pub const AUTO_PORT: u16 = 0;

const RANDOM_PORT_MIN: u16 = 10_000;
const RANDOM_PORT_MAX: u16 = 60_000;

pub fn is_avoided_port(port: u16) -> bool {
    port == 8080 || port == 8081
}

/// Ask the OS for an ephemeral port on `host` (bind to `:0`, then release).
pub fn pick_ephemeral_port(host: &str) -> std::io::Result<u16> {
    let ip: IpAddr = host
        .parse()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
    let listener = TcpListener::bind(SocketAddr::from((ip, 0)))?;
    Ok(listener.local_addr()?.port())
}

pub fn random_port_candidate() -> u16 {
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    RANDOM_PORT_MIN + (seed as u32 % (RANDOM_PORT_MAX - RANDOM_PORT_MIN) as u32) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_ephemeral_port_is_available() {
        let port = pick_ephemeral_port("127.0.0.1").unwrap();
        assert!(port > 0);
        assert!(!is_avoided_port(port));
        TcpListener::bind(("127.0.0.1", port)).unwrap();
    }
}
