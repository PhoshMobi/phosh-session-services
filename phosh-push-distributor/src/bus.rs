// SPDX-FileCopyrightText: 2026 Phosh.mobi e.V.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Talking *to* connectors: the `org.unifiedpush.Connector2` interface.
//!
//! All D-Bus emission goes through [`ConnectorBus`] so the rest of the
//! service can later be tested easily.

use std::collections::HashMap;

use async_trait::async_trait;
use zbus::Connection;
use zbus::message::{Flags, Message};
use zbus::zvariant::Value;

pub const CONNECTOR_PATH: &str = "/org/unifiedpush/Connector";
pub const CONNECTOR_INTERFACE: &str = "org.unifiedpush.Connector2";

/// The `a{sv}` argument dictionary all `UnifiedPush` calls use.
pub type Args = HashMap<&'static str, Value<'static>>;

#[async_trait]
pub trait ConnectorBus: Send + Sync {
    /// `Connector2.Message`: deliver a push message.
    ///
    /// # Errors
    ///
    /// Fails if the call could not be sent.
    async fn message(
        &self,
        service: &str,
        token: &str,
        payload: &[u8],
        id: Option<&str>,
    ) -> zbus::Result<()>;

    /// `Connector2.NewEndpoint`: tell the connector its endpoint.
    ///
    /// # Errors
    ///
    /// Fails if the call could not be sent.
    async fn new_endpoint(&self, service: &str, token: &str, endpoint: &str) -> zbus::Result<()>;

    /// `Connector2.Unregistered`: confirm an unregistration.
    ///
    /// # Errors
    ///
    /// Fails if the call could not be sent.
    async fn unregistered(&self, service: &str, token: &str) -> zbus::Result<()>;
}

#[must_use]
pub fn message_args(token: &str, payload: &[u8], id: Option<&str>) -> Args {
    let mut args = Args::new();
    args.insert("token", Value::from(token.to_string()));
    args.insert("message", Value::from(payload.to_vec()));
    if let Some(id) = id {
        args.insert("id", Value::from(id.to_string()));
    }
    args
}

#[must_use]
pub fn new_endpoint_args(token: &str, endpoint: &str) -> Args {
    let mut args = Args::new();
    args.insert("token", Value::from(token.to_string()));
    args.insert("endpoint", Value::from(endpoint.to_string()));
    args
}

#[must_use]
pub fn unregistered_args(token: &str) -> Args {
    let mut args = Args::new();
    args.insert("token", Value::from(token.to_string()));
    args
}

/// [`ConnectorBus`] on the session bus.
pub struct DbusBus {
    conn: Connection,
}

impl DbusBus {
    #[must_use]
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    /// The spec says callers must not wait for a reply, so these are
    /// fire-and-forget. This also means a slow to activate application
    /// can't block us.
    async fn call(&self, service: &str, member: &'static str, args: Args) -> zbus::Result<()> {
        let message = Message::method_call(CONNECTOR_PATH, member)?
            .interface(CONNECTOR_INTERFACE)?
            .destination(service)?
            .with_flags(Flags::NoReplyExpected)?
            .build(&args)?;
        self.conn.send(&message).await
    }
}

#[async_trait]
impl ConnectorBus for DbusBus {
    async fn message(
        &self,
        service: &str,
        token: &str,
        payload: &[u8],
        id: Option<&str>,
    ) -> zbus::Result<()> {
        self.call(service, "Message", message_args(token, payload, id))
            .await
    }

    async fn new_endpoint(&self, service: &str, token: &str, endpoint: &str) -> zbus::Result<()> {
        self.call(service, "NewEndpoint", new_endpoint_args(token, endpoint))
            .await
    }

    async fn unregistered(&self, service: &str, token: &str) -> zbus::Result<()> {
        self.call(service, "Unregistered", unregistered_args(token))
            .await
    }
}
