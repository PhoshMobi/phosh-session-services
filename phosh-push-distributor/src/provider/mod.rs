// SPDX-FileCopyrightText: 2026 Phosh.mobi e.V.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Push providers.
//!
//! A provider is the upstream push service (ntfy, Mozilla Push Service, …).
//! It hands out endpoints for connectors and delivers the pushes that arrive
//! on them. We call the service side "provider" as "connector" is for the
//! client side in UP terminology.
//!
//! To add a new provider:
//! - implement [`Provider`],
//! - add a table to [`crate::config::Config`]
//! - construct it in [`build`]
//!
//! Let's see how well this works when we add s 2nd provider

pub mod ntfy;

use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::config::Config;

/// What a connector asked for in `Register`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegisterRequest {
    pub token: String,
    pub service: String,
    pub description: Option<String>,
    pub vapid: Option<String>,
}

/// What a provider handed out for a registration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subscription {
    /// Provider side identifier (ntfy topic, Mozilla channel ID, …). This is
    /// sensitive since it's (part of) a capability URL: don't log it.
    pub remote_id: String,
    /// The push endpoint the connector hands to its application server.
    pub endpoint: String,
}

/// A push that arrived at a provider.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Incoming {
    pub remote_id: String,
    /// The raw push message, i.e. the body the application server `POST`ed.
    pub payload: Vec<u8>,
    /// Provider message id. Also used as resume marker (see
    /// [`Provider::subscribe`]).
    pub id: Option<String>,
}

/// Provider failures. Each variant maps onto one of the failure reasons of
/// `org.unifiedpush.Distributor2.Register`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Network(String),
    ActionRequired(String),
    VapidRequired,
    Internal(String),
}

impl Error {
    /// The `reason` string to return to the connector.
    #[must_use]
    pub fn reason(&self) -> &'static str {
        match self {
            Error::Network(_) => "NETWORK",
            Error::ActionRequired(_) => "ACTION_REQUIRED",
            Error::VapidRequired => "VAPID_REQUIRED",
            Error::Internal(_) => "INTERNAL_ERROR",
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Network(msg) => write!(f, "network failure: {msg}"),
            Error::ActionRequired(msg) => write!(f, "action required: {msg}"),
            Error::VapidRequired => write!(f, "a VAPID key is required"),
            Error::Internal(msg) => write!(f, "internal error: {msg}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<reqwest::Error> for Error {
    fn from(error: reqwest::Error) -> Self {
        // The URL contains the topic, which is a secret.
        Error::Network(error.without_url().to_string())
    }
}

#[async_trait]
pub trait Provider: Send + Sync {
    /// Name used in the config file and in the persisted registrations.
    fn name(&self) -> &'static str;

    /// Create a new subscription for a connector.
    ///
    /// # Errors
    ///
    /// Fails with the reason to report to the connector.
    async fn register(&self, request: &RegisterRequest) -> Result<Subscription, Error>;

    /// Drop the provider side of a subscription.
    ///
    /// # Errors
    ///
    /// Fails if the provider can't be told. The registration is dropped
    /// locally regardless.
    async fn unregister(&self, subscription: &Subscription) -> Result<(), Error>;

    /// Start delivering pushes for `subscription` to `out`. Must reconnect on
    /// its own. `cursor` is the [`Incoming::id`] of the last push that was
    /// delivered before (if any) so providers without server side queues can
    /// resume where they left off. Providers that don't need it ignore it.
    fn subscribe(
        &self,
        subscription: &Subscription,
        cursor: Option<String>,
        out: mpsc::Sender<Incoming>,
    );

    /// Stop delivering pushes for `subscription`.
    fn unsubscribe(&self, subscription: &Subscription);
}

/// Construct the providers enabled in `config`.
///
/// # Errors
///
/// Fails if a provider's configuration is invalid.
pub fn build(config: &Config) -> Result<Vec<Arc<dyn Provider>>, Box<dyn std::error::Error>> {
    let mut providers: Vec<Arc<dyn Provider>> = Vec::new();

    if let Some(ntfy) = &config.ntfy {
        providers.push(Arc::new(ntfy::Ntfy::new(ntfy)?));
    }

    if let Some(name) = &config.default_provider
        && !providers.iter().any(|p| p.name() == name)
    {
        return Err(format!("default_provider \"{name}\" is not configured").into());
    }

    Ok(providers)
}
