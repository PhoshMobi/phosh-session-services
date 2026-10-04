// SPDX-FileCopyrightText: 2026 Phosh.mobi e.V.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Persistent state.
//!
//! * [`Registry`]: which connector registered with which subscription,
//! * [`Cursors`]: the last message id delivered per registration, Allows us
//!   to prevent replaying old pushes on restart. Keyed by connector
//!   token.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use log::{debug, warn};
use serde::{Deserialize, Serialize};

use crate::provider::Subscription;
use crate::secrets::{self, RegistrationStore};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registration {
    /// The connector's D-Bus service name.
    pub service: String,
    pub provider: String,
    pub description: Option<String>,
    pub subscription: Subscription,
}

/// Registrations by connector token, mirroring one keyring item per token.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Registry {
    registrations: BTreeMap<String, Registration>,
}

impl Registry {
    /// Load registration items found in the store
    ///
    /// # Errors
    ///
    /// Fails if the keyring itself can't be reached or is locked.
    pub async fn load(store: &dyn RegistrationStore) -> Result<Self, secrets::Error> {
        let mut registrations = BTreeMap::new();
        for (token, json) in store.list_registrations().await? {
            match serde_json::from_str(&json) {
                Ok(registration) => {
                    registrations.insert(token, registration);
                }
                Err(error) => warn!("Ignoring corrupt registration for {token}: {error}"),
            }
        }
        Ok(Self { registrations })
    }

    /// Persist `token`'s current registration as its own item.
    ///
    /// # Errors
    ///
    /// Fails if the keyring can't be written.
    pub async fn save_one(
        &self,
        store: &dyn RegistrationStore,
        token: &str,
    ) -> Result<(), secrets::Error> {
        let Some(registration) = self.registrations.get(token) else {
            return Ok(());
        };
        let json = serde_json::to_string(registration)
            .map_err(|error| secrets::Error::new(error.to_string()))?;
        store.set_registration(token, &json).await
    }

    /// Drop `token`'s item from the store.
    ///
    /// # Errors
    ///
    /// Fails if the keyring can't be written.
    pub async fn delete_one(
        &self,
        store: &dyn RegistrationStore,
        token: &str,
    ) -> Result<(), secrets::Error> {
        store.delete_registration(token).await
    }

    #[must_use]
    pub fn get(&self, token: &str) -> Option<&Registration> {
        self.registrations.get(token)
    }

    pub fn insert(&mut self, token: String, registration: Registration) {
        self.registrations.insert(token, registration);
    }

    pub fn remove(&mut self, token: &str) -> Option<Registration> {
        self.registrations.remove(token)
    }

    /// Find the registration a push for `remote_id` belongs to.
    #[must_use]
    pub fn find_by_remote_id(&self, remote_id: &str) -> Option<(&str, &Registration)> {
        self.registrations
            .iter()
            .find(|(_, r)| r.subscription.remote_id == remote_id)
            .map(|(token, r)| (token.as_str(), r))
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &Registration)> {
        self.registrations.iter().map(|(t, r)| (t.as_str(), r))
    }
}

#[derive(Debug, Default)]
pub struct Cursors {
    path: Option<PathBuf>,
    by_token: HashMap<String, String>,
}

impl Cursors {
    /// Load cursors from `path`.
    pub async fn load(path: PathBuf) -> Self {
        let by_token = match tokio::fs::read_to_string(&path).await {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|error| {
                warn!("Ignoring broken {}: {error}", path.display());
                HashMap::new()
            }),
            Err(error) => {
                debug!("No cursors at {}: {error}", path.display());
                HashMap::new()
            }
        };

        Self {
            path: Some(path),
            by_token,
        }
    }

    #[must_use]
    pub fn get(&self, token: &str) -> Option<String> {
        self.by_token.get(token).cloned()
    }

    pub async fn set(&mut self, token: &str, id: &str) {
        self.by_token.insert(token.to_string(), id.to_string());
        self.save().await;
    }

    pub async fn remove(&mut self, token: &str) {
        if self.by_token.remove(token).is_some() {
            self.save().await;
        }
    }

    async fn save(&self) {
        let Some(path) = &self.path else {
            return;
        };
        let Ok(json) = serde_json::to_string(&self.by_token) else {
            return;
        };

        let result = async {
            if let Some(dir) = path.parent() {
                tokio::fs::create_dir_all(dir).await?;
            }
            // Write + rename so a crash can't leave a half written file
            let tmp = path.with_extension("tmp");
            tokio::fs::write(&tmp, json).await?;
            tokio::fs::rename(&tmp, path).await
        }
        .await;

        if let Err(error) = result {
            warn!("Failed to save cursors to {}: {error}", path.display());
        }
    }
}
