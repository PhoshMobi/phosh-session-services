// SPDX-FileCopyrightText: 2026 Phosh.mobi e.V.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Provider independent core: keeps track of registrations and turns
//! pushes arriving at providers into `Connector2` calls.

use std::collections::HashMap;
use std::sync::Arc;

use log::{debug, info, trace, warn};
use tokio::sync::{Mutex, mpsc};
use zbus::names::WellKnownName;

use crate::bus::ConnectorBus;
use crate::provider::{self, Incoming, Provider, RegisterRequest};
use crate::registry::{Cursors, Registration, Registry};
use crate::secrets::{self, RegistrationStore};

/// From the `UnifiedPush` D-Bus specification.
const MAX_TOKEN_LEN: usize = 100;
const MAX_DESCRIPTION_LEN: usize = 100;
const VAPID_LEN: usize = 87;
const MAX_PAYLOAD_LEN: usize = 4096;

#[derive(Debug)]
pub enum RegisterError {
    Invalid(String),
    Provider(provider::Error),
    Storage(secrets::Error),
}

impl RegisterError {
    #[must_use]
    pub fn reason(&self) -> &'static str {
        match self {
            RegisterError::Invalid(_) | RegisterError::Storage(_) => "INTERNAL_ERROR",
            RegisterError::Provider(error) => error.reason(),
        }
    }
}

impl std::fmt::Display for RegisterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegisterError::Invalid(msg) => write!(f, "invalid request: {msg}"),
            RegisterError::Provider(error) => error.fmt(f),
            RegisterError::Storage(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for RegisterError {}

impl From<provider::Error> for RegisterError {
    fn from(error: provider::Error) -> Self {
        RegisterError::Provider(error)
    }
}

impl From<secrets::Error> for RegisterError {
    fn from(error: secrets::Error) -> Self {
        RegisterError::Storage(error)
    }
}

struct State {
    registry: Registry,
    cursors: Cursors,
}

pub struct Distributor {
    bus: Arc<dyn ConnectorBus>,
    providers: HashMap<String, Arc<dyn Provider>>,
    default_provider: Option<String>,
    registrations: Arc<dyn RegistrationStore>,
    state: Mutex<State>,
    incoming: mpsc::Sender<Incoming>,
}

fn validate(request: &RegisterRequest) -> Result<(), RegisterError> {
    let invalid = |msg: &str| Err(RegisterError::Invalid(msg.to_string()));

    if request.token.is_empty() || request.token.len() > MAX_TOKEN_LEN {
        return invalid("token must be [1, 100] bytes");
    }
    if WellKnownName::try_from(request.service.as_str()).is_err() {
        return invalid("service is not a well-known bus name");
    }
    if request
        .description
        .as_ref()
        .is_some_and(|d| d.len() > MAX_DESCRIPTION_LEN)
    {
        return invalid("description is >100 bytes");
    }
    if request.vapid.as_ref().is_some_and(|v| v.len() != VAPID_LEN) {
        return invalid("vapid must be 87 bytes");
    }
    Ok(())
}

impl Distributor {
    /// Create the distributor. Providers push into the returned receiver,
    /// hand it to [`Distributor::run`].
    #[must_use]
    pub fn new(
        bus: Arc<dyn ConnectorBus>,
        providers: Vec<Arc<dyn Provider>>,
        default_provider: Option<String>,
        registrations: Arc<dyn RegistrationStore>,
        registry: Registry,
        cursors: Cursors,
    ) -> (Arc<Self>, mpsc::Receiver<Incoming>) {
        let (incoming, rx) = mpsc::channel(32);
        let default_provider =
            default_provider.or_else(|| providers.first().map(|p| p.name().to_string()));
        let providers = providers
            .into_iter()
            .map(|p| (p.name().to_string(), p))
            .collect();

        let distributor = Arc::new(Self {
            bus,
            providers,
            default_provider,
            registrations,
            state: Mutex::new(State { registry, cursors }),
            incoming,
        });
        (distributor, rx)
    }

    /// Re-subscribe all persisted registrations. Called once at startup.
    pub async fn resume(&self) {
        let state = self.state.lock().await;
        for (token, registration) in state.registry.iter() {
            let Some(provider) = self.providers.get(&registration.provider) else {
                warn!(
                    "Provider {} of a registration is not configured anymore",
                    registration.provider
                );
                continue;
            };
            provider.subscribe(
                &registration.subscription,
                state.cursors.get(token),
                self.incoming.clone(),
            );
        }
        info!("Resumed {} registration(s)", state.registry.iter().count());
    }

    /// Handle `Distributor2.Register`. Returns the endpoint. The caller then
    /// replies to the connector and calls [`Distributor::announce_endpoint`].
    ///
    /// # Errors
    ///
    /// Fails if the request is malformed, the provider refuses or the new
    /// registration can't be stored.
    pub async fn register(&self, request: RegisterRequest) -> Result<String, RegisterError> {
        validate(&request)?;

        let mut state = self.state.lock().await;

        if let Some(existing) = state.registry.get(&request.token) {
            if existing.service != request.service {
                return Err(RegisterError::Invalid("token is in use".into()));
            }
            return Ok(existing.subscription.endpoint.clone());
        }

        let provider = self
            .default_provider
            .as_ref()
            .and_then(|name| self.providers.get(name))
            .ok_or_else(|| provider::Error::ActionRequired("no push provider configured".into()))?;

        let subscription = provider.register(&request).await?;
        let endpoint = subscription.endpoint.clone();

        state.registry.insert(
            request.token.clone(),
            Registration {
                service: request.service.clone(),
                provider: provider.name().to_string(),
                description: request.description.clone(),
                subscription: subscription.clone(),
            },
        );
        if let Err(error) = state
            .registry
            .save_one(self.registrations.as_ref(), &request.token)
            .await
        {
            state.registry.remove(&request.token);
            if let Err(error) = provider.unregister(&subscription).await {
                debug!("Cleaning up failed registration: {error}");
            }
            return Err(error.into());
        }

        provider.subscribe(&subscription, None, self.incoming.clone());
        info!("Registered {} ({})", request.service, provider.name());
        Ok(endpoint)
    }

    /// Send `Connector2.NewEndpoint` for a registered token.
    pub async fn announce_endpoint(&self, token: &str) {
        let target = {
            let state = self.state.lock().await;
            state
                .registry
                .get(token)
                .map(|r| (r.service.clone(), r.subscription.endpoint.clone()))
        };
        let Some((service, endpoint)) = target else {
            return;
        };

        if let Err(error) = self.bus.new_endpoint(&service, token, &endpoint).await {
            debug!("NewEndpoint to {service} failed: {error}");
        }
    }

    /// Handle `Distributor2.Unregister`.
    pub async fn unregister(&self, token: &str) {
        let removed = {
            let mut state = self.state.lock().await;
            let Some(registration) = state.registry.remove(token) else {
                return;
            };
            state.cursors.remove(token).await;
            if let Err(error) = state
                .registry
                .delete_one(self.registrations.as_ref(), token)
                .await
            {
                warn!("Failed to remove registration from the keyring: {error}");
            }
            registration
        };

        if let Some(provider) = self.providers.get(&removed.provider) {
            provider.unsubscribe(&removed.subscription);
            if let Err(error) = provider.unregister(&removed.subscription).await {
                debug!("Provider unregistration failed: {error}");
            }
        }

        info!("Unregistered {}", removed.service);
        if let Err(error) = self.bus.unregistered(&removed.service, token).await {
            debug!("Unregistered to {} failed: {error}", removed.service);
        }
    }

    /// Deliver a push that arrived at a provider to its connector.
    pub async fn handle_incoming(&self, push: Incoming) {
        if push.payload.is_empty() || push.payload.len() > MAX_PAYLOAD_LEN {
            debug!("Dropping push of {} bytes", push.payload.len());
            return;
        }

        let target = {
            let state = self.state.lock().await;
            state
                .registry
                .find_by_remote_id(&push.remote_id)
                .map(|(token, r)| (token.to_string(), r.service.clone()))
        };
        let Some((token, service)) = target else {
            debug!("Dropping push for unknown subscription");
            return;
        };

        trace!("Pushing for {service}");
        let sent = self
            .bus
            .message(&service, &token, &push.payload, push.id.as_deref())
            .await;
        if let Err(error) = sent {
            warn!("Message to {service} failed: {error}");
            return;
        }

        if let Some(id) = &push.id {
            self.state.lock().await.cursors.set(&token, id).await;
        }
    }

    /// Deliver pushes until all providers are gone.
    pub async fn run(self: Arc<Self>, mut pushes: mpsc::Receiver<Incoming>) {
        while let Some(push) = pushes.recv().await {
            self.handle_incoming(push).await;
        }
    }
}
