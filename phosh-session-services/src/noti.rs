// SPDX-FileCopyrightText: 2026 Phosh.mobi e.V.
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use futures_lite::stream::StreamExt;
use log::{debug, trace};
use zbus::zvariant::Value;
use zbus::{Connection, Result, proxy};

const DEFAULT_TIMEOUT_MS: i32 = 5000;

#[proxy(
    interface = "org.freedesktop.Notifications",
    default_service = "org.freedesktop.Notifications",
    default_path = "/org/freedesktop/Notifications"
)]
trait Notification {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: &[&str],
        hints: HashMap<&str, &Value<'_>>,
        expire_timeout: i32,
    ) -> zbus::Result<u32>;

    #[zbus(signal)]
    fn action_invoked(&self, id: u32, action: &str) -> zbus::Result<()>;
}

#[derive(Debug)]
pub enum Event {
    ActionInvoked { id: u32, action: String },
    // TODO: Handle NotificationClosed too
}

#[derive(Default)]
pub struct Noti {
    icon: Option<String>,
    summary: Option<String>,
    body: Option<String>,
    actions: Option<Vec<String>>,
}

impl Noti {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    #[must_use]
    pub fn summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    #[must_use]
    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }

    #[must_use]
    pub fn actions(mut self, actions: Vec<String>) -> Self {
        self.actions = Some(actions);
        self
    }
}

#[derive(Clone)]
pub struct NotiManager {
    conn: Connection,
    app_id: String,
    ids: Arc<Mutex<HashSet<u32>>>,
}

impl NotiManager {
    /// # Errors
    ///
    /// Will return `Err` if the connection to the `DBus` session bus fails
    pub async fn new(app_id: impl Into<String>) -> Result<Self> {
        Ok(Self {
            conn: Connection::session().await?,
            app_id: app_id.into(),
            ids: Arc::new(Mutex::new(HashSet::new())),
        })
    }

    /// # Errors
    ///
    /// Will return `Err` if the `DBus` calls fail
    ///
    /// # Panics
    ///
    /// Will panic another thread trying to get the `ids` mutex panic'ed
    pub async fn notify(
        &mut self,
        noti: Noti,
        timeout: Option<i32>,
        replaces_id: Option<u32>,
    ) -> Result<u32> {
        let proxy = NotificationProxy::new(&self.conn).await?;
        let mut hints = HashMap::new();
        hints.insert("urgency", &Value::U8(1));
        let s = Value::from(self.app_id.as_str());
        hints.insert("desktop-entry", &s);
        let replaces_id = replaces_id.unwrap_or(0);

        let actions: Vec<&str> = noti
            .actions
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(String::as_str)
            .collect();
        let id = proxy
            .notify(
                &self.app_id,
                replaces_id,
                noti.icon.as_deref().unwrap_or(""),
                noti.summary.as_deref().unwrap_or(""),
                noti.body.as_deref().unwrap_or(""),
                &actions,
                hints,
                timeout.unwrap_or(DEFAULT_TIMEOUT_MS),
            )
            .await?;
        self.ids.lock().unwrap().insert(id);
        Ok(id)
    }

    /// # Errors
    ///
    /// Will return `Err` if the `DBus` calls fail
    ///
    /// # Panics
    ///
    /// Will panic another thread trying to get the `ids` mutex panic'ed
    pub async fn monitor<F>(&self, mut callback: F) -> Result<()>
    where
        F: FnMut(Event) + Send + 'static,
    {
        let proxy = NotificationProxy::new(&self.conn).await?;
        let mut signals = proxy.receive_action_invoked().await?;

        while let Some(signal) = signals.next().await {
            let args = signal.args()?;

            let is_known = self.ids.lock().unwrap().remove(&args.id);
            if !is_known {
                trace!("Notification with id {} is not for us", args.id);
                return Ok(());
            }

            debug!(
                "Handling notification id {}, action {}",
                args.id, args.action
            );
            callback(Event::ActionInvoked {
                id: args.id,
                action: args.action.to_owned(),
            });
        }

        Ok(())
    }
}
