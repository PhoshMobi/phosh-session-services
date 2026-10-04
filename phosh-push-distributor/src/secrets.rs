// SPDX-FileCopyrightText: 2026 Phosh.mobi e.V.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Secret storage. Keeps e.g. ntfy topics as they're the "secrets"
//! on public servers.

use std::collections::HashMap;
use std::fmt;

use async_trait::async_trait;
use log::debug;
use oo7::Secret;

/// `application` attribute on all keyring items.
pub const APPLICATION: &str = "mobi.phosh.PushDistributor";

#[derive(Debug)]
pub struct Error(String);

impl Error {
    pub fn new(msg: impl Into<String>) -> Self {
        Self(msg.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "secret storage failed: {}", self.0)
    }
}

impl std::error::Error for Error {}

impl From<oo7::Error> for Error {
    fn from(error: oo7::Error) -> Self {
        Self(error.to_string())
    }
}

/// Storage for connector registrations: one item per token.
#[async_trait]
pub trait RegistrationStore: Send + Sync {
    /// # Errors
    ///
    /// Fails if the store can't be reached or is locked.
    async fn list_registrations(&self) -> Result<Vec<(String, String)>, Error>;

    /// # Errors
    ///
    /// Fails if the store can't be reached or is locked.
    async fn set_registration(&self, token: &str, value: &str) -> Result<(), Error>;

    /// # Errors
    ///
    /// Fails if the store can't be reached or is locked.
    async fn delete_registration(&self, token: &str) -> Result<(), Error>;
}

/// The Secret Service
pub struct Keyring {
    keyring: oo7::Keyring,
}

impl Keyring {
    /// # Errors
    ///
    /// Fails if there's no Secret Service on the session bus.
    pub async fn new() -> Result<Self, Error> {
        let keyring = oo7::Keyring::new().await?;
        if let Err(error) = keyring.unlock().await {
            debug!("Unlocking keyring failed: {error}");
        }
        Ok(Self { keyring })
    }
}

fn registration_attributes(token: &str) -> HashMap<&str, &str> {
    HashMap::from([
        ("application", APPLICATION),
        ("kind", "registration"),
        ("token", token),
    ])
}

fn item_secret(secret: &oo7::Secret) -> Result<String, Error> {
    String::from_utf8(secret.as_bytes().to_vec())
        .map_err(|_| Error::new("secret is not valid UTF-8"))
}

#[async_trait]
impl RegistrationStore for Keyring {
    async fn list_registrations(&self) -> Result<Vec<(String, String)>, Error> {
        let query = HashMap::from([("application", APPLICATION), ("kind", "registration")]);
        let items = self.keyring.search_items(&query).await?;

        let mut registrations = Vec::with_capacity(items.len());
        for item in items {
            let Some(token) = item.attributes().await?.remove("token") else {
                debug!("Ignoring a registration item without a token attribute");
                continue;
            };
            registrations.push((token, item_secret(&item.secret().await?)?));
        }
        Ok(registrations)
    }

    async fn set_registration(&self, token: &str, value: &str) -> Result<(), Error> {
        let label = format!("Phosh push distributor: registration {token}");
        self.keyring
            .create_item(
                &label,
                &registration_attributes(token),
                Secret::text(value),
                true,
            )
            .await?;
        Ok(())
    }

    async fn delete_registration(&self, token: &str) -> Result<(), Error> {
        self.keyring.delete(&registration_attributes(token)).await?;
        Ok(())
    }
}
