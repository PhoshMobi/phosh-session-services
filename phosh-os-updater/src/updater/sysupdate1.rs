// SPDX-FileCopyrightText: 2026 Phosh.mobi e.V.
// SPDX-License-Identifier: GPL-3.0-or-later

use log::{trace, warn};
use serde::Deserialize;
use zbus::{Connection, Result, proxy};
use zvariant::{OwnedObjectPath, Type};

use crate::updater::{UpdateError, UpdateInfo};

#[cfg(all(feature = "systemd-dbus-sysupdate1-session", not(debug_assertions)))]
compile_error!("'systemd-dbus-sysupdate1-session' is for development only.");

impl From<zbus::Error> for UpdateError {
    fn from(err: zbus::Error) -> Self {
        Self::Failed(format!("sysupdate1 D-Bus error: {err}"))
    }
}

impl From<serde_json::Error> for UpdateError {
    fn from(err: serde_json::Error) -> Self {
        Self::Failed(format!("Json decode error: {err}"))
    }
}

#[derive(Debug, Deserialize)]
struct TargetVersion {
    version: String,
    newest: bool,
    available: bool,
    installed: bool,
}

#[derive(Debug, Deserialize, Type)]
struct Target {
    class: String,
    name: String,
    path: OwnedObjectPath,
}

#[proxy(
    interface = "org.freedesktop.sysupdate1.Manager",
    default_service = "org.freedesktop.sysupdate1",
    default_path = "/org/freedesktop/sysupdate1"
)]
trait Sysupdate1Manager {
    #[zbus(allow_interactive_auth)]
    async fn list_targets(&self) -> zbus::Result<Vec<Target>>;
}

#[proxy(
    interface = "org.freedesktop.sysupdate1.Target",
    default_service = "org.freedesktop.sysupdate1"
)]
trait Sysupdate1Target {
    #[zbus(property)]
    fn path(&self) -> zbus::Result<OwnedObjectPath>;

    #[zbus(allow_interactive_auth)]
    async fn list(&self, flags: u64) -> zbus::Result<Vec<String>>;
    #[zbus(allow_interactive_auth)]
    async fn describe(&self, version: &str, flags: u64) -> zbus::Result<String>;
}

pub struct Sysupdate1UpdateChecker {
    conn: Connection,
}

impl Sysupdate1UpdateChecker {
    /// # Errors
    ///
    /// Will return `Err` if the connection to the `DBus` system bus fails
    pub async fn new() -> Result<Self> {
        #[cfg(feature = "systemd-dbus-sysupdate1-session")]
        let conn = Connection::session().await?;

        #[cfg(not(feature = "systemd-dbus-sysupdate1-session"))]
        let conn = Connection::system().await?;

        Ok(Self { conn })
    }

    /// # Errors
    ///
    /// Will return `Err` if the `DBus` calls fail
    pub async fn check_for_updates(&self) -> std::result::Result<Option<UpdateInfo>, UpdateError> {
        let sysupdate = Sysupdate1ManagerProxy::new(&self.conn).await?;

        let targets = sysupdate.list_targets().await?;
        for target in targets {
            match self.check_target(&target).await {
                Ok(Some(update)) => {
                    return Ok(Some(update));
                }

                Ok(None) => {}

                Err(err) => {
                    warn!("Failed to check for update of {}: {}", target.path, err);
                }
            }
        }

        Ok(None)
    }

    async fn check_target(
        &self,
        target: &Target,
    ) -> std::result::Result<Option<UpdateInfo>, UpdateError> {
        trace!("Target {}/{}/{}", target.class, target.name, target.path);

        if target.class != "host" || target.name != "host" {
            return Ok(None);
        }

        let proxy = Sysupdate1TargetProxy::builder(&self.conn)
            .path(&target.path)?
            .build()
            .await?;

        let versions = proxy.list(0).await?;

        for version in versions {
            let json = proxy.describe(&version, 0).await?;

            let v: TargetVersion = serde_json::from_str(&json)?;

            // New system update available
            if v.newest && v.available && !v.installed {
                return Ok(Some(UpdateInfo {
                    version: v.version,
                    description: "New system update available".into(),
                }));
            }
        }

        Ok(None)
    }
}
