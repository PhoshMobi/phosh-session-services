use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use reqwest::{Client, Method, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::RwLock;
use url::Url;

use crate::Folder;
use crate::config::address_to_url;

const EVENTS: &str =
    "ConfigSaved,FolderCompletion,FolderPaused,FolderResumed,FolderSummary,StateChanged";

#[allow(clippy::cast_possible_truncation)]
fn get_completion(need: u64, global: u64) -> u8 {
    let completion = 100 - (need * 100).checked_div(global).unwrap_or(0);
    completion as u8
}

pub enum Update {
    Folders(Vec<Folder>),
    Folder {
        id: String,
        completion: Option<u8>,
        state: Option<String>,
        paused: Option<bool>,
    },
    Gui {
        url: Url,
    },
    None,
}

#[derive(Debug)]
pub enum Error {
    Authorization(String),
    Connection(String),
    Deserialization(String),
    Unknown(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Authorization(msg) => write!(f, "authorization failed: {msg}"),
            Error::Connection(msg) => write!(f, "connection failed: {msg}"),
            Error::Deserialization(msg) => write!(f, "deserialization failed: {msg}"),
            Error::Unknown(msg) => write!(f, "unknown failure: {msg}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<Error> for zbus::Error {
    fn from(error: Error) -> Self {
        let details = error.to_string();
        Self::Failure(details)
    }
}

impl From<Error> for zbus::fdo::Error {
    fn from(error: Error) -> Self {
        let error = zbus::Error::from(error);
        Self::from(error)
    }
}

impl From<reqwest::Error> for Error {
    fn from(error: reqwest::Error) -> Self {
        let msg = error.to_string();
        if let Some(status) = error.status() {
            if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
                Error::Authorization(msg)
            } else if status.is_server_error() {
                Error::Connection(msg)
            } else {
                Error::Unknown(msg)
            }
        } else if error.is_connect() || error.is_decode() || error.is_timeout() {
            Error::Connection(msg)
        } else {
            Error::Unknown(msg)
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        let msg = error.to_string();
        Error::Deserialization(msg)
    }
}

#[derive(Debug, Deserialize)]
struct Event {
    id: u64,
    #[serde(flatten)]
    payload: PayloadType,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum PayloadType {
    Known(Payload),
    Unknown { r#type: String, data: Value },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", content = "data")]
enum Payload {
    ConfigSaved {
        gui: GuiConfig,
    },
    StateChanged {
        folder: String,
        to: String,
    },
    FolderCompletion {
        folder: String,
        completion: f64,
    },
    FolderSummary {
        folder: String,
        summary: SummaryData,
    },
    FolderPaused {
        id: String,
    },
    FolderResumed {
        id: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GuiConfig {
    address: String,
    api_key: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FolderConfig {
    id: String,
    label: String,
    path: String,
    paused: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SummaryData {
    global_bytes: u64,
    need_bytes: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FolderStatus {
    global_bytes: u64,
    need_bytes: u64,
    state: String,
}

#[derive(Clone)]
pub struct Syncthing {
    api_key: Arc<RwLock<String>>,
    url: Arc<RwLock<Url>>,
    client: Client,
    last_id: Arc<AtomicU64>,
}

impl Syncthing {
    #[must_use]
    #[allow(clippy::missing_errors_doc)]
    pub fn new(api_key: &str, url: &Url) -> Self {
        Syncthing {
            api_key: Arc::new(RwLock::new(api_key.to_string())),
            url: Arc::new(RwLock::new(url.clone())),
            client: Client::new(),
            last_id: Arc::default(),
        }
    }

    async fn prepare_request(&self, method: Method, path: &str) -> reqwest::RequestBuilder {
        let mut url = self.url.read().await.clone();
        url.set_path(path);
        let lock = self.api_key.read().await;
        self.client.request(method, url).bearer_auth(&*lock)
    }

    async fn send_request(&self, builder: reqwest::RequestBuilder) -> Result<String, Error> {
        tracing::debug!("Sending request: {builder:#?}");
        let response = builder.send().await?.error_for_status()?;
        tracing::debug!("Response: {}", response.status());
        let text = response.text().await?;
        tracing::debug!("\n{}", ellipsize(&text));
        Ok(text)
    }

    #[allow(clippy::missing_errors_doc)]
    pub async fn set_paused(&self, folder_id: &str, paused: bool) -> Result<(), Error> {
        tracing::debug!("Setting Paused of folder {folder_id}: {paused}");
        let path = format!("/rest/config/folders/{folder_id}");
        let data = json!({"paused": paused});
        let builder = self.prepare_request(Method::PATCH, &path).await.json(&data);
        self.send_request(builder).await?;
        Ok(())
    }

    #[allow(clippy::missing_errors_doc)]
    async fn get_folders(&self) -> Result<Vec<Folder>, Error> {
        let builder = self
            .prepare_request(Method::GET, "/rest/config/folders")
            .await;
        let text = self.send_request(builder).await?;
        let configs: Vec<FolderConfig> = serde_json::from_str(&text)?;

        let mut folders = Vec::with_capacity(configs.len());

        for config in configs {
            let builder = self
                .prepare_request(reqwest::Method::GET, "/rest/db/status")
                .await
                .query(&[("folder", &config.id)]);
            let text = self.send_request(builder).await?;
            let status: FolderStatus = serde_json::from_str(&text)?;
            let completion = get_completion(status.need_bytes, status.global_bytes);
            let folder = Folder::new(
                config.id,
                config.label,
                config.path,
                completion,
                status.state,
                config.paused,
                self.clone(),
            );
            folders.push(folder);
        }

        Ok(folders)
    }

    async fn set_config(&self, config: GuiConfig) -> Result<Option<Url>, Error> {
        tracing::debug!("Refreshing GUI configuration");

        let url = match address_to_url(&config.address) {
            Ok(url) => url,
            Err(error) => return Err(Error::Unknown(error.to_string())),
        };

        let mut lock = self.api_key.write().await;
        *lock = config.api_key;

        let changed = {
            let lock = self.url.read().await;
            url != *lock
        };

        if changed {
            let mut lock = self.url.write().await;
            *lock = url.clone();
            return Ok(Some(url));
        }

        Ok(None)
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    async fn on_response(&self, text: String) -> Result<Vec<Update>, Error> {
        type FolderData = (Option<u8>, Option<String>, Option<bool>);

        let events: Vec<Event> = serde_json::from_str(&text)?;
        if events.is_empty() {
            return Ok(vec![Update::None]);
        }

        let last_id = self.last_id.load(Ordering::Relaxed);
        let oldest_event_id = events[0].id;
        let latest_id = events[events.len() - 1].id;
        self.last_id.store(latest_id, Ordering::Relaxed);

        if last_id != 0 && (oldest_event_id - last_id) != 1 {
            tracing::info!("Refreshing to match missed events from {last_id} to {oldest_event_id}");
            let folders = self.get_folders().await?;
            return Ok(vec![Update::Folders(folders)]);
        }

        let mut config_saved = false;
        let mut new_url = None;
        let mut folders: HashMap<String, FolderData> = HashMap::new();

        for event in events {
            tracing::debug!("New event:\n{}", ellipsize(&format!("{event:#?}")));
            match event.payload {
                PayloadType::Known(payload) => match payload {
                    Payload::ConfigSaved { gui } => {
                        new_url = self.set_config(gui).await?;
                        config_saved = true;
                        break;
                    }
                    Payload::FolderCompletion {
                        folder: id,
                        completion,
                    } => {
                        let completion = completion as u8;
                        if let Some(data) = folders.get_mut(&id) {
                            data.0 = Some(completion);
                        } else {
                            folders.insert(id, (Some(completion), None, None));
                        }
                    }
                    Payload::FolderPaused { id } => {
                        if let Some(data) = folders.get_mut(&id) {
                            data.2 = Some(true);
                        } else {
                            folders.insert(id, (None, None, Some(true)));
                        }
                    }
                    Payload::FolderResumed { id } => {
                        if let Some(data) = folders.get_mut(&id) {
                            data.2 = Some(false);
                        } else {
                            folders.insert(id, (None, None, Some(false)));
                        }
                    }
                    Payload::FolderSummary {
                        folder: id,
                        summary,
                    } => {
                        let completion = get_completion(summary.need_bytes, summary.global_bytes);
                        if let Some(data) = folders.get_mut(&id) {
                            data.0 = Some(completion);
                        } else {
                            folders.insert(id, (Some(completion), None, None));
                        }
                    }
                    Payload::StateChanged { folder: id, to } => {
                        if let Some(data) = folders.get_mut(&id) {
                            data.1 = Some(to);
                        } else {
                            folders.insert(id, (None, Some(to), None));
                        }
                    }
                },
                PayloadType::Unknown { r#type, data } => {
                    panic!("Unknown payload: {type}: {data}");
                }
            }
        }

        if config_saved {
            let folders = self.get_folders().await?;
            return Ok(vec![Update::Folders(folders)]);
        }

        let mut updates: Vec<Update> = folders
            .drain()
            .map(|(id, data)| Update::Folder {
                id,
                completion: data.0,
                state: data.1,
                paused: data.2,
            })
            .collect();

        if let Some(url) = new_url {
            updates.push(Update::Gui { url });
        }

        Ok(updates)
    }

    #[allow(clippy::missing_errors_doc)]
    pub async fn poll(&self, bootstrap: bool) -> Result<Vec<Update>, Error> {
        if bootstrap {
            tracing::info!("Bootstrapping as requested");
            self.last_id.store(0, Ordering::Relaxed);
            let folders = self.get_folders().await?;
            return Ok(vec![Update::Folders(folders)]);
        }

        let mut builder = self
            .prepare_request(Method::GET, "/rest/events")
            .await
            .query(&[("since", self.last_id.load(Ordering::Relaxed))])
            .query(&[("events", EVENTS)]);
        builder = if self.last_id.load(Ordering::Relaxed) == 0 {
            builder.query(&[("limit", 1)])
        } else {
            builder
        };
        let text = self.send_request(builder).await?;
        self.on_response(text).await
    }
}

fn ellipsize(text: &str) -> String {
    if text.len() < 80 {
        String::from(text)
    } else {
        format!("{}…", &text[..80])
    }
}
