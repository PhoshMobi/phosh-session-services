// SPDX-FileCopyrightText: 2026 Phosh.mobi e.V.
// SPDX-License-Identifier: GPL-3.0-or-later

// gdbus call --session --dest mobi.phosh.MobileSettings
//     --object-path /mobi/phosh/MobileSettings
//     --method org.gtk.Actions.Activate "set-panel"
//       "[<('overview',[<'hidden-apps'>])>]" "{}"

use std::collections::HashMap;

use zbus::{Connection, Result, proxy};
use zvariant::Value;

#[proxy(
    interface = "org.gtk.Actions",
    default_service = "mobi.phosh.MobileSettings",
    default_path = "/mobi/phosh/MobileSettings"
)]
trait GtkActions {
    async fn activate(
        &self,
        action_name: &str,
        parameter: Vec<Value<'_>>,
        platform_data: HashMap<String, Value<'_>>,
    ) -> zbus::Result<()>;
}

// Make it easy to open settings panels
pub struct MobileSettingsPanel {
    conn: Connection,
}

impl MobileSettingsPanel {
    /// # Errors
    ///
    /// Will return `Err` if the connection to the `DBus` session bus fails
    pub async fn new() -> Result<Self> {
        Ok(Self {
            conn: Connection::session().await?,
        })
    }

    /// # Errors
    ///
    /// Will return `Err` if the `DBus` calls fail
    pub async fn open_panel(&self, panel: &str, subpage: Option<&str>) -> Result<()> {
        let parameter = match subpage {
            Some(subpage) => vec![Value::from((panel, vec![subpage]))],
            None => vec![Value::from((panel, Vec::<Value<'_>>::new()))],
        };

        let proxy = GtkActionsProxy::new(&self.conn).await?;
        proxy
            .activate("set-panel", parameter, HashMap::new())
            .await?;
        Ok(())
    }
}
