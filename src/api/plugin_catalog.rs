//! Bounded, authenticated option sources for isolated plugin clients.
use crate::{auth::AuthenticatedToken, error::AppError};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, header},
};
use serde::{Deserialize, Serialize};
use sonicboom::tts::providers::ProviderStatusState;

#[derive(Deserialize)]
pub struct VoiceQuery {
    provider: Option<String>,
}

/// `?provider=` narrows the voice list to one provider's `provider:id`
/// entries. Absent or empty keeps the full catalog (backward compatible);
/// the value is capped so it can only ever be a short id prefix.
fn provider_prefix(provider: Option<&str>) -> Option<String> {
    let provider = provider?.trim();
    if provider.is_empty() {
        return None;
    }
    Some(format!(
        "{}:",
        provider.chars().take(64).collect::<String>()
    ))
}

#[derive(Serialize)]
pub struct OptionItem {
    id: String,
    name: String,
    disabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    state: Option<String>,
}
fn headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, "private, no-store".parse().unwrap());
    headers
}
pub async fn providers(
    _token: AuthenticatedToken,
    State(state): State<crate::AppState>,
) -> (HeaderMap, Json<Vec<OptionItem>>) {
    let registry = super::providers::build_registry_for_state(&state).await;
    let items = registry
        .statuses()
        .into_iter()
        .filter(|s| s.enabled)
        .map(|s| {
            let label = match s.provider_id.as_str() {
                "supertonic" => "Supertonic",
                "reverb-tts" => "Reverb-TTS",
                _ => "Unknown provider",
            };
            let ready = s.state == ProviderStatusState::Ready;
            OptionItem {
                id: s.provider_id,
                name: if ready {
                    label.into()
                } else {
                    format!("{label} ({})", s.state.as_str())
                },
                disabled: !ready,
                state: Some(s.state.as_str().into()),
            }
        })
        .collect();
    (headers(), Json(items))
}
pub async fn voices(
    token: AuthenticatedToken,
    State(state): State<crate::AppState>,
    axum::extract::Query(query): axum::extract::Query<VoiceQuery>,
) -> Result<(HeaderMap, Json<Vec<OptionItem>>), AppError> {
    let registry = super::providers::build_registry_for_state(&state).await;
    let config = state.config.get().await;
    let owner = token.rate_limit_key();
    let profiles = super::voices::with_store(&state, move |store| {
        Ok(store
            .list()
            .into_iter()
            .filter(|p| p.owner_token_hash.as_deref() == Some(owner.as_str()))
            .cloned()
            .collect::<Vec<_>>())
    })
    .await?;
    let mut items = Vec::new();
    if let Some(handle) = super::providers::supertonic_handle(&state).await {
        for name in handle.voice_styles.keys() {
            items.push(OptionItem {
                id: format!("supertonic:{name}"),
                name: name.clone(),
                disabled: false,
                state: None,
            });
        }
    }
    for profile in profiles {
        let Some(provider) = registry.get(&profile.provider) else {
            continue;
        };
        let status = provider.status();
        let capabilities = provider.capabilities();
        if !status.enabled
            || status.state != ProviderStatusState::Ready
            || status.active_model.as_deref() != Some(profile.model.as_str())
            || profile.model_revision
                != super::voices::config_revision_for(&config, &profile.provider)
            || !capabilities.models.contains(&profile.model)
            || !capabilities.languages.contains(&profile.language)
            || !capabilities.clone_modes.contains(&profile.clone_mode)
        {
            continue;
        }
        items.push(OptionItem {
            id: format!("{}:{}", profile.provider, profile.id),
            name: profile.name.chars().take(128).collect(),
            disabled: false,
            state: None,
        });
    }
    if let Some(prefix) = provider_prefix(query.provider.as_deref()) {
        items.retain(|item| item.id.starts_with(&prefix));
    }
    items.sort_by(|a, b| a.id.cmp(&b.id));
    items.truncate(500);
    Ok((headers(), Json(items)))
}

#[cfg(test)]
mod tests {
    use super::provider_prefix;

    #[test]
    fn provider_prefix_filters_and_bounds() {
        assert_eq!(provider_prefix(None), None);
        assert_eq!(provider_prefix(Some("  ")), None);
        assert_eq!(
            provider_prefix(Some("supertonic")).as_deref(),
            Some("supertonic:")
        );
        assert_eq!(
            provider_prefix(Some("p".repeat(100).as_str())).as_deref(),
            Some(format!("{}:", "p".repeat(64))).as_deref()
        );
    }
}
