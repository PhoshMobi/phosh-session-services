// SPDX-FileCopyrightText: 2026 Phosh.mobi e.V.
// SPDX-License-Identifier: GPL-3.0-or-later

//! [ntfy](https://ntfy.sh) provider.
//!
//! An endpoint is `{server}/{topic}?up=1` with a random, unguessable topic
//! (the topic is the only access control on a public server, so it's kept in
//! the keyring and never logged). Pushes arrive on one long-lived HTTPS
//! request per topic: `GET {server}/{topic}/json` streams one JSON object per
//! line. With HTTP/2 these requests share a single connection.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use log::{debug, warn};
use reqwest::{Client, StatusCode, Url};
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{sleep, timeout};

use super::{Error, Incoming, Provider, RegisterRequest, Subscription};
use crate::config::NtfyConfig;

const NAME: &str = "ntfy";

/// 160 bits, as the `UnifiedPush` spec recommends for endpoints.
const TOPIC_BYTES: usize = 20;
const TOPIC_PREFIX: &str = "up";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
/// ntfy sends a keepalive every 45s by default.
const IDLE_TIMEOUT: Duration = Duration::from_mins(3);
const BACKOFF_MIN: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_mins(5);
/// A JSON line is far smaller than this, so we're being fed garbage.
const MAX_LINE_LEN: usize = 64 * 1024;

struct Inner {
    client: Client,
    /// Without trailing slash.
    server: String,
}

pub struct Ntfy {
    inner: Arc<Inner>,
    tasks: Mutex<HashMap<String, JoinHandle<()>>>,
}

impl Ntfy {
    /// # Errors
    ///
    /// Fails if the server URL is invalid or the HTTP client can't be set up.
    pub fn new(config: &NtfyConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let url = Url::parse(&config.server)
            .map_err(|error| format!("[ntfy] server \"{}\": {error}", config.server))?;
        if url.scheme() != "https" && url.scheme() != "http" {
            return Err(format!("[ntfy] server \"{}\" must be http(s)", config.server).into());
        }
        if url.scheme() == "http" {
            warn!("ntfy server is not using TLS, pushes are exposed");
        }

        let client = Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .user_agent(concat!(
                "phosh-push-distributor/",
                env!("CARGO_PKG_VERSION")
            ))
            .build()?;

        Ok(Self {
            inner: Arc::new(Inner {
                client,
                server: url.as_str().trim_end_matches('/').to_string(),
            }),
            tasks: Mutex::new(HashMap::new()),
        })
    }
}

fn random_topic() -> String {
    let bytes: [u8; TOPIC_BYTES] = rand::random();
    format!("{TOPIC_PREFIX}{}", URL_SAFE_NO_PAD.encode(bytes))
}

impl Inner {
    /// Follow one topic until the connection ends.
    async fn stream_once(
        &self,
        topic: &str,
        cursor: &mut Option<String>,
        out: &mpsc::Sender<Incoming>,
        backoff: &mut Duration,
    ) -> Result<(), Error> {
        let mut url = format!("{}/{topic}/json", self.server);
        if let Some(cursor) = cursor {
            url.push_str("?since=");
            url.push_str(cursor);
        }

        let mut response = self.client.get(url).send().await?;
        let status = response.status();
        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            return Err(Error::ActionRequired(format!("server said {status}")));
        }
        if status.is_client_error() && cursor.take().is_some() {
            // Most likely the message we resume from expired
            return Err(Error::Internal(format!(
                "server said {status}, dropping cursor"
            )));
        }
        if !status.is_success() {
            return Err(Error::Network(format!("server said {status}")));
        }

        let mut lines = LineBuffer::default();
        loop {
            let chunk = timeout(IDLE_TIMEOUT, response.chunk())
                .await
                .map_err(|_| Error::Network("stream went quiet".into()))??;
            let Some(chunk) = chunk else {
                return Ok(());
            };

            for line in lines.push(&chunk) {
                match parse_event(&line) {
                    Some(Event::Open) => *backoff = BACKOFF_MIN,
                    Some(Event::Message { id, payload }) => {
                        let push = Incoming {
                            remote_id: topic.to_string(),
                            payload,
                            id: Some(id.clone()),
                        };
                        if out.send(push).await.is_err() {
                            return Ok(());
                        }
                        *cursor = Some(id);
                    }
                    Some(Event::Keepalive) | None => {}
                }
            }
        }
    }

    /// Follow `topic` forever, reconnecting with backoff.
    async fn follow(
        self: Arc<Self>,
        topic: String,
        mut cursor: Option<String>,
        out: mpsc::Sender<Incoming>,
    ) {
        let mut backoff = BACKOFF_MIN;
        loop {
            // The error can mention the server, but never the topic
            if let Err(error) = self
                .stream_once(&topic, &mut cursor, &out, &mut backoff)
                .await
            {
                warn!("ntfy stream failed: {error}");
            } else {
                debug!("ntfy stream ended");
            }
            if out.is_closed() {
                return;
            }
            sleep(backoff).await;
            backoff = (backoff * 2).min(BACKOFF_MAX);
        }
    }
}

#[async_trait]
impl Provider for Ntfy {
    fn name(&self) -> &'static str {
        NAME
    }

    async fn register(&self, _request: &RegisterRequest) -> Result<Subscription, Error> {
        // Subscribing itself needs no round trip: topics are created by
        // using them.
        let topic = random_topic();
        let endpoint = format!("{}/{topic}?up=1", self.inner.server);
        Ok(Subscription {
            remote_id: topic,
            endpoint,
        })
    }

    async fn unregister(&self, _subscription: &Subscription) -> Result<(), Error> {
        // Nothing to tell the server, the topic just goes unused
        Ok(())
    }

    fn subscribe(
        &self,
        subscription: &Subscription,
        cursor: Option<String>,
        out: mpsc::Sender<Incoming>,
    ) {
        let topic = subscription.remote_id.clone();
        let task = tokio::spawn(Arc::clone(&self.inner).follow(topic.clone(), cursor, out));
        if let Some(old) = self.tasks.lock().unwrap().insert(topic, task) {
            old.abort();
        }
    }

    fn unsubscribe(&self, subscription: &Subscription) {
        if let Some(task) = self.tasks.lock().unwrap().remove(&subscription.remote_id) {
            task.abort();
        }
    }
}

impl Drop for Ntfy {
    fn drop(&mut self) {
        for task in self.tasks.lock().unwrap().values() {
            task.abort();
        }
    }
}

/// Splits a byte stream into lines.
#[derive(Default)]
struct LineBuffer(Vec<u8>);

impl LineBuffer {
    fn push(&mut self, data: &[u8]) -> Vec<String> {
        self.0.extend_from_slice(data);

        let mut lines = Vec::new();
        while let Some(end) = self.0.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.0.drain(..=end).collect();
            lines.push(String::from_utf8_lossy(&line[..end]).into_owned());
        }
        if self.0.len() > MAX_LINE_LEN {
            self.0.clear();
        }
        lines
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Event {
    Open,
    Keepalive,
    Message { id: String, payload: Vec<u8> },
}

#[derive(Deserialize)]
struct RawEvent {
    event: String,
    id: Option<String>,
    message: Option<String>,
    encoding: Option<String>,
}

/// Parse one line of the JSON stream. Events we don't care about
/// (`message_delete`, `poll_request`, …) and garbage give `None`.
fn parse_event(line: &str) -> Option<Event> {
    let raw: RawEvent = serde_json::from_str(line.trim()).ok()?;

    match raw.event.as_str() {
        "open" => Some(Event::Open),
        "keepalive" => Some(Event::Keepalive),
        "message" => {
            let id = raw.id?;
            let message = raw.message?;
            // ntfy base64 encodes bodies that aren't valid UTF-8
            let payload = if raw.encoding.as_deref() == Some("base64") {
                STANDARD.decode(message).ok()?
            } else {
                message.into_bytes()
            };
            Some(Event::Message { id, payload })
        }
        _ => None,
    }
}
