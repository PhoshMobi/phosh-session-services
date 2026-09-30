// SPDX-FileCopyrightText: 2026 Phosh.mobi e.V.
// SPDX-License-Identifier: GPL-3.0-or-later

use futures_lite::stream::StreamExt;
use log::{debug, trace, warn};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Metered {
    Unknown = 0,
    Yes = 1,
    No = 2,
    GuessYes = 3,
    GuessNo = 4,
}

impl TryFrom<u32> for Metered {
    type Error = u32;

    fn try_from(value: u32) -> std::result::Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Unknown),
            1 => Ok(Self::Yes),
            2 => Ok(Self::No),
            3 => Ok(Self::GuessYes),
            4 => Ok(Self::GuessNo),
            value => Err(value),
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

    #[zbus(property)]
    fn metered(&self) -> zbus::Result<u32>;
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
        F: FnMut(Connectivity, Metered) + Send + 'static,
    {
        let nm = NetworkManagerProxy::new(&self.conn).await?;

        let props = zbus::fdo::PropertiesProxy::builder(&self.conn)
            .destination("org.freedesktop.NetworkManager")?
            .path(nm.inner().path())?
            .build()
            .await?;

        // Initial sync
        let initial = nm.connectivity().await?;
        let mut connectivity = match Connectivity::try_from(initial) {
            Ok(c) => c,
            Err(v) => {
                debug!("Unknown connectivity value {v}");
                Connectivity::Unknown
            }
        };

        let initial = nm.metered().await?;
        let mut metered = match Metered::try_from(initial) {
            Ok(m) => m,
            Err(v) => {
                debug!("Unknown metered value {v}");
                Metered::Unknown
            }
        };
        trace!("Initial metered: {metered:?}");
        callback(connectivity, metered);

        // Listen for property changes
        let mut props_changed = props.receive_properties_changed().await?;
        while let Some(signal) = props_changed.next().await {
            let args = signal.args()?;
            let mut notify = false;

            if args.interface_name() != "org.freedesktop.NetworkManager" {
                continue;
            }

            if let Some(value) = args.changed_properties().get("Connectivity") {
                let val: u32 = value.downcast_ref()?;
                match Connectivity::try_from(val) {
                    Ok(new_connectivity) => {
                        if connectivity != new_connectivity {
                            debug!("Connectivity changed: {new_connectivity:?}");
                            connectivity = new_connectivity;
                            notify = true;
                        }
                    }
                    Err(v) => {
                        warn!("Unknown connectivity value {v}");
                    }
                }
            }

            if let Some(value) = args.changed_properties().get("Metered") {
                let val: u32 = value.downcast_ref()?;
                match Metered::try_from(val) {
                    Ok(new_metered) => {
                        if metered != new_metered {
                            debug!("Metered changed: {new_metered:?}");
                            metered = new_metered;
                            notify = true;
                        }
                    }
                    Err(v) => {
                        warn!("Unknown metered value {v}");
                    }
                }
            }

            if notify {
                callback(connectivity, metered);
            }
        }

        Ok(())
    }
}
