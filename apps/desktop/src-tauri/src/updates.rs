use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, time::Duration};
use tauri::{Manager, WebviewWindow};
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_updater::UpdaterExt;

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ReleaseChannel {
    Stable,
    Nightly,
}

impl ReleaseChannel {
    fn endpoint(self) -> &'static str {
        match self {
            Self::Stable => {
                "https://github.com/Popchor-RUB/Scorewing/releases/latest/download/latest.json"
            }
            Self::Nightly => {
                "https://github.com/Popchor-RUB/Scorewing/releases/download/nightly/latest.json"
            }
        }
    }

    fn release_page(self) -> &'static str {
        match self {
            Self::Stable => "https://github.com/Popchor-RUB/Scorewing/releases/latest",
            Self::Nightly => "https://github.com/Popchor-RUB/Scorewing/releases/tag/nightly",
        }
    }
}

pub fn require_main(window: &WebviewWindow) -> Result<(), String> {
    if window.label() == "main" {
        Ok(())
    } else {
        Err("Only the main window can manage updates.".into())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateEnvironment {
    current_version: String,
    supported: bool,
}

#[tauri::command]
pub fn update_environment(window: WebviewWindow) -> Result<UpdateEnvironment, String> {
    require_main(&window)?;
    #[cfg(target_os = "linux")]
    let supported = window.env().appimage.is_some();
    #[cfg(not(target_os = "linux"))]
    let supported = true;
    Ok(UpdateEnvironment {
        current_version: window.package_info().version.to_string(),
        supported,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMetadata {
    rid: tauri::ResourceId,
    current_version: String,
    version: String,
    body: Option<String>,
    raw_json: serde_json::Value,
    downgrade: bool,
}

fn should_offer(order: Ordering, manual: bool) -> bool {
    order == Ordering::Greater || (manual && order == Ordering::Less)
}

#[tauri::command]
pub async fn check_update(
    window: WebviewWindow,
    channel: ReleaseChannel,
    manual: bool,
) -> Result<Option<UpdateMetadata>, String> {
    require_main(&window)?;
    let updater = window
        .updater_builder()
        .endpoints(vec![channel
            .endpoint()
            .parse()
            .map_err(|error| format!("{error}"))?])
        .map_err(|error| error.to_string())?
        .timeout(Duration::from_secs(15))
        .version_comparator(move |current, remote| {
            should_offer(remote.version.cmp(&current), manual)
        })
        .build()
        .map_err(|error| error.to_string())?;
    let Some(update) = updater.check().await.map_err(|error| error.to_string())? else {
        return Ok(None);
    };
    let downgrade = window.package_info().version.cmp(
        &update
            .version
            .parse()
            .map_err(|error| format!("Invalid update version: {error}"))?,
    ) == Ordering::Greater;
    let metadata = UpdateMetadata {
        current_version: update.current_version.clone(),
        version: update.version.clone(),
        body: update.body.clone(),
        raw_json: update.raw_json.clone(),
        // Reuse the plugin resource so its signed download/install API remains authoritative.
        rid: window.resources_table().add(update),
        downgrade,
    };
    Ok(Some(metadata))
}

#[tauri::command]
pub fn open_update_downloads(window: WebviewWindow, channel: ReleaseChannel) -> Result<(), String> {
    require_main(&window)?;
    window
        .opener()
        .open_url(channel.release_page(), None::<&str>)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_checks_only_offer_newer_versions_and_manual_checks_can_downgrade() {
        assert!(should_offer(Ordering::Greater, false));
        assert!(!should_offer(Ordering::Equal, false));
        assert!(!should_offer(Ordering::Less, false));
        assert!(should_offer(Ordering::Greater, true));
        assert!(!should_offer(Ordering::Equal, true));
        assert!(should_offer(Ordering::Less, true));
    }

    #[test]
    fn channels_only_select_the_expected_https_release_endpoints() {
        assert!(matches!(
            serde_json::from_str::<ReleaseChannel>("\"nightly\"").unwrap(),
            ReleaseChannel::Nightly
        ));
        assert!(serde_json::from_str::<ReleaseChannel>("\"https://example.com\"").is_err());
        assert!(ReleaseChannel::Stable
            .endpoint()
            .ends_with("/releases/latest/download/latest.json"));
        assert!(ReleaseChannel::Nightly
            .endpoint()
            .ends_with("/releases/download/nightly/latest.json"));
        for channel in [ReleaseChannel::Stable, ReleaseChannel::Nightly] {
            assert_eq!(
                tauri::Url::parse(channel.endpoint()).unwrap().host_str(),
                Some("github.com")
            );
            assert_eq!(
                tauri::Url::parse(channel.release_page()).unwrap().scheme(),
                "https"
            );
        }
    }
}
