use serde_json::json;
use tower_lsp::lsp_types::notification::Notification;
use tower_lsp::lsp_types::{MessageActionItem, MessageType};

use super::Backend;

/// Tells the client to add the host to the allowed hosts setting
enum AllowHost {}

impl Notification for AllowHost {
    type Params = serde_json::Value;
    const METHOD: &'static str = "tomlSchema/allowHost";
}

impl Backend {
    /// Remote schema hosts are allowed by the settings, or by the user when asked for the first time
    pub(super) async fn is_host_allowed(&self, host: &str) -> bool {
        {
            let settings = self.remote_settings.read().unwrap();
            if !settings.enabled {
                return false;
            }

            if settings.is_host_allowed(host) {
                return true;
            }
        }

        // One prompt per host, concurrent validations wait for the answer
        let slot = self
            .host_decisions
            .lock()
            .unwrap()
            .entry(host.to_ascii_lowercase())
            .or_default()
            .clone();
        let mut decision = slot.lock().await;
        if let Some(allowed) = *decision {
            return allowed;
        }

        let action = |title: &str| MessageActionItem {
            title: title.to_string(),
            properties: Default::default(),
        };
        let answer = self
            .client
            .show_message_request(
                MessageType::WARNING,
                format!("Allow fetching TOML schemas from {host}?"),
                Some(vec![action("Allow"), action("Deny")]),
            )
            .await;

        // A dismissed prompt, or a client which cannot show it, counts as denied
        let allowed = matches!(answer, Ok(Some(item)) if item.title == "Allow");
        *decision = Some(allowed);

        if allowed {
            self.client
                .send_notification::<AllowHost>(json!({ "host": host }))
                .await;
        }
        allowed
    }
}
