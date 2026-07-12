//! Discover Bedrock text/chat models available to the current AWS credentials.

use anyhow::{Context, Result};
use futures_util::stream::{self, StreamExt};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct BedrockModelInfo {
    pub id: String,
    pub label: String,
    pub provider: String,
    /// `foundation` | `inference_profile` | `mantle`
    pub kind: String,
}

fn has_aws_credentials() -> bool {
    std::env::var("AWS_ACCESS_KEY_ID")
        .ok()
        .filter(|k| !k.is_empty())
        .is_some()
        || std::env::var("AWS_PROFILE")
            .ok()
            .filter(|p| !p.is_empty())
            .is_some()
        || std::env::var("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI").is_ok()
        || std::env::var("AWS_WEB_IDENTITY_TOKEN_FILE").is_ok()
}

/// Claude models usable via Bedrock Mantle (Anthropic Messages API).
pub fn mantle_claude_models() -> Vec<BedrockModelInfo> {
    [
        (
            "anthropic.claude-3-5-sonnet-20241022-v2:0",
            "Claude 3.5 Sonnet v2",
        ),
        (
            "anthropic.claude-3-5-haiku-20241022-v1:0",
            "Claude 3.5 Haiku",
        ),
        ("anthropic.claude-3-opus-20240229-v1:0", "Claude 3 Opus"),
        (
            "us.anthropic.claude-sonnet-4-5-20250929-v1:0",
            "Claude Sonnet 4.5 (US inference profile)",
        ),
        (
            "us.anthropic.claude-opus-4-20250514-v1:0",
            "Claude Opus 4 (US inference profile)",
        ),
        (
            "us.anthropic.claude-haiku-4-5-20251001-v1:0",
            "Claude Haiku 4.5 (US inference profile)",
        ),
    ]
    .into_iter()
    .map(|(id, name)| BedrockModelInfo {
        id: id.into(),
        label: format!("Anthropic · {name}"),
        provider: "Anthropic".into(),
        kind: "mantle".into(),
    })
    .collect()
}

fn is_text_chat_foundation(summary: &aws_sdk_bedrock::types::FoundationModelSummary) -> bool {
    let has_text_in = summary
        .input_modalities()
        .iter()
        .any(|x| matches!(x, aws_sdk_bedrock::types::ModelModality::Text));
    let has_text_out = summary
        .output_modalities()
        .iter()
        .any(|x| matches!(x, aws_sdk_bedrock::types::ModelModality::Text));
    if !has_text_in || !has_text_out {
        return false;
    }
    let id = summary.model_id().to_ascii_lowercase();
    if id.contains("embed")
        || id.contains("image")
        || id.contains("tts")
        || id.contains("speech")
        || id.contains("rerank")
        || id.contains("transcri")
    {
        return false;
    }
    true
}

async fn foundation_model_authorized(
    client: &aws_sdk_bedrock::Client,
    model_id: &str,
) -> bool {
    match client
        .get_foundation_model_availability()
        .model_id(model_id)
        .send()
        .await
    {
        Ok(avail) => matches!(
            avail.authorization_status(),
            aws_sdk_bedrock::types::AuthorizationStatus::Authorized
        ),
        Err(e) => {
            // Don't hide models when the availability API is denied/unsupported.
            tracing::debug!("bedrock availability check failed for {model_id}: {e:#}");
            true
        }
    }
}

/// List text/chat models for the configured region using IAM credentials when present.
/// With only a Mantle API key (no AWS creds), returns Claude models Mantle can serve.
pub async fn list_bedrock_models(
    region: &str,
    mantle_key_present: bool,
) -> Result<Vec<BedrockModelInfo>> {
    let region = region.trim();
    let region = if region.is_empty() { "us-east-1" } else { region };

    if !has_aws_credentials() {
        if mantle_key_present {
            return Ok(mantle_claude_models());
        }
        anyhow::bail!(
            "No AWS credentials found. Set AWS_ACCESS_KEY_ID / AWS_PROFILE (or a Bedrock Mantle key for Claude-only)."
        );
    }

    let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(aws_config::Region::new(region.to_string()))
        .load()
        .await;
    let client = aws_sdk_bedrock::Client::new(&config);

    let mut candidates: Vec<BedrockModelInfo> = Vec::new();
    let mut seen = std::collections::HashSet::<String>::new();

    match client.list_inference_profiles().max_results(1000).send().await {
        Ok(resp) => {
            for p in resp.inference_profile_summaries() {
                let id = p.inference_profile_id();
                if id.is_empty() || !seen.insert(id.to_string()) {
                    continue;
                }
                let name = p.inference_profile_name();
                let name = if name.is_empty() { id } else { name };
                candidates.push(BedrockModelInfo {
                    id: id.to_string(),
                    label: format!("Profile · {name}"),
                    provider: "Inference profile".into(),
                    kind: "inference_profile".into(),
                });
            }
        }
        Err(e) => {
            tracing::warn!("bedrock list_inference_profiles failed: {e:#}");
        }
    }

    let foundation = client
        .list_foundation_models()
        .by_output_modality(aws_sdk_bedrock::types::ModelModality::Text)
        .by_inference_type(aws_sdk_bedrock::types::InferenceType::OnDemand)
        .send()
        .await
        .context("bedrock list_foundation_models failed")?;

    for summary in foundation.model_summaries() {
        if !is_text_chat_foundation(summary) {
            continue;
        }
        let id = summary.model_id();
        if id.is_empty() || !seen.insert(id.to_string()) {
            continue;
        }
        let provider = summary
            .provider_name()
            .filter(|s| !s.is_empty())
            .unwrap_or("Bedrock")
            .to_string();
        let name = summary
            .model_name()
            .filter(|s| !s.is_empty())
            .unwrap_or(id);
        candidates.push(BedrockModelInfo {
            id: id.to_string(),
            label: format!("{provider} · {name}"),
            provider,
            kind: "foundation".into(),
        });
    }

    // Filter foundation models to ones this account is authorized to use.
    let foundation_ids: Vec<String> = candidates
        .iter()
        .filter(|m| m.kind == "foundation")
        .map(|m| m.id.clone())
        .collect();

    let authorized: std::collections::HashSet<String> = stream::iter(foundation_ids)
        .map(|id| {
            let client = client.clone();
            async move {
                if foundation_model_authorized(&client, &id).await {
                    Some(id)
                } else {
                    None
                }
            }
        })
        .buffer_unordered(10)
        .filter_map(|x| async move { x })
        .collect()
        .await;

    let mut out: Vec<BedrockModelInfo> = candidates
        .into_iter()
        .filter(|m| m.kind != "foundation" || authorized.contains(&m.id))
        .collect();

    if out.is_empty() && mantle_key_present {
        return Ok(mantle_claude_models());
    }

    out.sort_by(|a, b| {
        a.provider
            .cmp(&b.provider)
            .then_with(|| a.label.cmp(&b.label))
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(out)
}

pub fn is_anthropic_bedrock_model(model_id: &str) -> bool {
    let lower = model_id.to_ascii_lowercase();
    lower.contains("anthropic.") || lower.contains("/anthropic.")
}
