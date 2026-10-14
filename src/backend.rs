use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use serde_json::Value;
use tower_lsp::Client;

mod consent;
mod lsp;
mod remote;
mod util;
mod validation;

/// User settings under `tomlSchema.remote`
#[derive(Default)]
struct RemoteSettings {
    enabled: bool,
    allowed_hosts: Vec<String>,
}

impl RemoteSettings {
    /// Accepts the `tomlSchema` section itself, or an object containing it
    fn from_json(value: &Value) -> Self {
        let section = value.get("tomlSchema").unwrap_or(value);
        let remote = section.get("remote");
        Self {
            enabled: remote
                .and_then(|r| r.get("enabled"))
                .and_then(Value::as_bool)
                .unwrap_or(true),
            allowed_hosts: remote
                .and_then(|r| r.get("allowedHosts"))
                .and_then(Value::as_array)
                .map(|hosts| {
                    hosts
                        .iter()
                        .filter_map(Value::as_str)
                        .map(|h| h.trim().to_ascii_lowercase())
                        .filter(|h| !h.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    fn set_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    fn is_host_allowed(&self, host: &str) -> bool {
        self.enabled
            && self
                .allowed_hosts
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(host))
    }
}

pub struct Backend {
    client: Client,
    remote_settings: RwLock<RemoteSettings>,
    remote_schemas: remote::RemoteSchemas,
    // The answers to the prompts for hosts which are not in the settings, until the server restarts
    host_decisions: Mutex<HashMap<String, Arc<tokio::sync::Mutex<Option<bool>>>>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            remote_settings: RwLock::new(RemoteSettings::default().set_enabled(true)),
            remote_schemas: remote::RemoteSchemas::default(),
            host_decisions: Mutex::new(HashMap::new()),
        }
    }

    fn update_settings(&self, value: &Value) {
        *self.remote_settings.write().unwrap() = RemoteSettings::from_json(value);
    }
}
