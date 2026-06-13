// SPDX-FileCopyrightText: 2026 Phosh.mobi e.V.
// SPDX-License-Identifier: GPL-3.0-or-later

use futures_lite::stream::StreamExt;
use log::{debug, trace};
use zbus::{Connection, Result, proxy};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Connectivity {
    Unknown,
    None,
    Portal,
    Limited,
    Full,
}

impl TryFrom<u32> for Connectivity {
    type Error = u32;

    fn try_from(value: u32) -> std::result::Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Unknown),
            1 => Ok(Self::None),
            2 => Ok(Self::Portal),
            3 => Ok(Self::Limited),
            4 => Ok(Self::Full),
            other => Err(other),
        }
    }
}

#[proxy(
    interface = "org.freedesktop.NetworkManager",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager"
)]
trait NetworkManager {
    #[zbus(property)]
    fn connectivity(&self) -> zbus::Result<u32>;
}

#[derive(Clone)]
pub struct NetworkManager {
    conn: Connection,
}

impl NetworkManager {
    /// # Errors
    ///
    /// Will return `Err` if the connection to the `DBus` system bus fails
    pub async fn new() -> Result<Self> {
        Ok(Self {
            conn: Connection::system().await?,
        })
    }

    /// # Errors
    ///
    /// Will return `Err` if the `DBus` calls fail
    pub async fn monitor_connectivity<F>(&self, mut callback: F) -> Result<()>
    where
        F: FnMut(Connectivity) + Send + 'static,
    {
        let nm = NetworkManagerProxy::new(&self.conn).await?;

        let props = zbus::fdo::PropertiesProxy::builder(&self.conn)
            .destination("org.freedesktop.NetworkManager")?
            .path(nm.inner().path())?
            .build()
            .await?;

        // Initial sync
        let initial = nm.connectivity().await?;
        let mut current = match Connectivity::try_from(initial) {
            Ok(c) => c,
            Err(v) => {
                debug!("Unknown connectivity value {v}");
                Connectivity::Unknown
            }
        };
        trace!("Initial connectivity: {current:?}");
        callback(current);

        // Listen for property changes
        let mut props_changed = props.receive_properties_changed().await?;
        while let Some(signal) = props_changed.next().await {
            let args = signal.args()?;

            if let Some(value) = args.changed_properties().get("Connectivity") {
                let val: u32 = value.downcast_ref()?;

                let new = match Connectivity::try_from(val) {
                    Ok(c) => c,
                    Err(v) => {
                        debug!("Unknown connectivity value {v}");
                        continue;
                    }
                };

                trace!("Connectivity changed: {new:?}");

                if current != new {
                    current = new;
                    callback(new);
                }
            }
        }

        Ok(())
    }
}
