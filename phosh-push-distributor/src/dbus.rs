// SPDX-FileCopyrightText: 2026 Phosh.mobi e.V.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Talking *from* connectors: the `org.unifiedpush.Distributor2` interface.

use std::collections::HashMap;
use std::hash::BuildHasher;
use std::sync::Arc;

use log::warn;
use zbus::zvariant::{OwnedValue, Value};
use zbus::{fdo, interface};

use crate::distributor::{Distributor, RegisterError};
use crate::provider::RegisterRequest;

pub const BUS_NAME: &str = "org.unifiedpush.Distributor.phosh";
pub const PATH: &str = "/org/unifiedpush/Distributor";

fn string_arg<S: BuildHasher>(args: &HashMap<String, OwnedValue, S>, key: &str) -> Option<String> {
    args.get(key)?
        .downcast_ref::<&str>()
        .ok()
        .map(str::to_owned)
}

/// Parse the `a{sv}` argument of `Register`.
#[must_use]
pub fn parse_register<S: BuildHasher>(
    args: &HashMap<String, OwnedValue, S>,
) -> Option<RegisterRequest> {
    Some(RegisterRequest {
        token: string_arg(args, "token")?,
        service: string_arg(args, "service")?,
        description: string_arg(args, "description"),
        vapid: string_arg(args, "vapid"),
    })
}

/// Build the `a{sv}` reply of `Register`.
#[must_use]
pub fn register_reply(failure: Option<&RegisterError>) -> HashMap<String, Value<'static>> {
    let mut reply = HashMap::new();
    match failure {
        None => {
            reply.insert("success".into(), Value::from("REGISTRATION_SUCCEEDED"));
        }
        Some(error) => {
            reply.insert("success".into(), Value::from("REGISTRATION_FAILED"));
            reply.insert("reason".into(), Value::from(error.reason()));
        }
    }
    reply
}

pub struct DistributorInterface {
    distributor: Arc<Distributor>,
}

impl DistributorInterface {
    #[must_use]
    pub fn new(distributor: Arc<Distributor>) -> Self {
        Self { distributor }
    }
}

#[interface(name = "org.unifiedpush.Distributor2")]
impl DistributorInterface {
    async fn register(
        &self,
        args: HashMap<String, OwnedValue>,
    ) -> fdo::Result<HashMap<String, Value<'static>>> {
        let request = parse_register(&args)
            .ok_or_else(|| fdo::Error::InvalidArgs("token and service are required".into()))?;
        let token = request.token.clone();

        match self.distributor.register(request).await {
            Ok(_) => {
                // The connector expects NewEndpoint after the reply
                let distributor = Arc::clone(&self.distributor);
                tokio::spawn(async move {
                    tokio::task::yield_now().await;
                    distributor.announce_endpoint(&token).await;
                });
                Ok(register_reply(None))
            }
            Err(RegisterError::Invalid(msg)) => Err(fdo::Error::InvalidArgs(msg)),
            Err(error) => {
                warn!("Registration failed: {error}");
                Ok(register_reply(Some(&error)))
            }
        }
    }

    async fn unregister(
        &self,
        args: HashMap<String, OwnedValue>,
    ) -> fdo::Result<HashMap<String, Value<'static>>> {
        let token = string_arg(&args, "token")
            .ok_or_else(|| fdo::Error::InvalidArgs("token is required".into()))?;
        self.distributor.unregister(&token).await;
        Ok(HashMap::new())
    }
}
