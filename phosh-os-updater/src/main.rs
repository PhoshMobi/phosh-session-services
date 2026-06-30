// SPDX-FileCopyrightText: 2026 Phosh.mobi e.V.
// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::{Duration, Instant};

use clap::Parser;
use gettextrs::{bind_textdomain_codeset, bindtextdomain, gettext, setlocale, textdomain};
use log::{debug, info, trace};
use phosh_os_updater::config;
use phosh_os_updater::updater::{UpdateChecker, UpdateInfo};
use phosh_session_services::{nm, noti, pms};
use tokio::time::sleep;
use zbus::Result;

struct Service {
    nm: nm::NetworkManager,
    noti_manager: noti::NotiManager,
    update_checker: UpdateChecker,
    noti_id: Option<u32>,
    update: Option<UpdateInfo>,
}

impl Service {
    pub async fn new(app_id: &str) -> Result<Self> {
        let network_manager = nm::NetworkManager::new().await?;

        Ok(Self {
            nm: network_manager,
            noti_manager: noti::NotiManager::new(app_id).await?,
            update_checker: UpdateChecker::new().await?,
            noti_id: None,
            update: None,
        })
    }

    fn start_nm_monitor(&self) -> async_channel::Receiver<nm::Connectivity> {
        let (tx, rx) = async_channel::bounded(1);

        let nm = self.nm.clone();
        tokio::spawn(async move {
            nm.monitor_connectivity(move |c| {
                let _ = tx.try_send(c);
            })
            .await
            .unwrap();
        });

        rx
    }

    fn start_noti_monitor(&self) -> async_channel::Receiver<noti::Event> {
        let (tx, rx) = async_channel::bounded(1);

        let noti = self.noti_manager.clone();
        tokio::spawn(async move {
            noti.monitor(move |c| {
                let _ = tx.try_send(c);
            })
            .await
            .unwrap();
        });

        rx
    }

    async fn notify_update(&mut self) -> Result<()> {
        let update = self.update.as_ref().unwrap();
        let msg = gettext("Update to {} available").replace("{}", &update.version);
        let noti = noti::Noti::new()
            .summary(gettext("OS Update available"))
            .actions(vec!["install.update".into(), gettext("Install Update")])
            .body(&msg);
        let id = self.noti_manager.notify(noti, None, self.noti_id).await?;

        trace!("Notification {} for {} send", id, update.version);
        self.noti_id = Some(id);

        Ok(())
    }

    async fn maybe_notify_update(&mut self) -> Result<()> {
        trace!("Checking for updates…");
        match self.update_checker.check_for_updates().await {
            Ok(Some(update)) => {
                info!("Update available: {update:?}");

                if self
                    .update
                    .as_ref()
                    .is_some_and(|current| current.version == update.version)
                {
                    return Ok(());
                }

                self.update = Some(update);
                let _ = self.notify_update().await;
            }

            Ok(None) => {
                debug!("No updates available");
                self.update = None;
            }

            Err(err) => {
                debug!("Failed to check for updates: {err}");
            }
        }

        Ok(())
    }
}

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {}

// Make sure we dont' check too often when connections change
fn should_check_updates(last_check: Option<Instant>) -> bool {
    match last_check {
        None => true,
        Some(t) => t.elapsed() >= Duration::from_hours(4),
    }
}

fn random_delay() -> Duration {
    let hours = rand::random_range(4..=12);

    Duration::from_secs(hours * 60 * 60)
}

fn i18n_init() {
    setlocale(gettextrs::LocaleCategory::LcAll, "");
    bindtextdomain(config::GETTEXT_PACKAGE, config::LOCALEDIR)
        .expect("Unable to bind the text domain");
    bind_textdomain_codeset(config::GETTEXT_PACKAGE, "UTF-8")
        .expect("Unable to set the text domain encoding");
    textdomain(config::GETTEXT_PACKAGE).expect("Unable to switch to the text domain");
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    i18n_init();

    let _args = Args::parse();
    let mut app = Service::new("mobi.phosh.OsUpdater").await?;
    let nm_rx = app.start_nm_monitor();
    let noti_rx = app.start_noti_monitor();
    let mut connectivity = nm::Connectivity::Unknown;
    let mut next_check = Box::pin(sleep(random_delay()));
    let mut last_update_check = None;

    trace!("Entering listening loop…");
    loop {
        tokio::select! {
            Ok(new) = nm_rx.recv() => {
                let old = connectivity;
                connectivity = new;

                if old != nm::Connectivity::Full
                && connectivity == nm::Connectivity::Full
                && should_check_updates(last_update_check) {
                    let _ = app.maybe_notify_update().await;
                    last_update_check = Some(Instant::now());
                    next_check = Box::pin(sleep(random_delay()));
                }
            }

            Ok(event) = noti_rx.recv() => {
                match event {
                    noti::Event::ActionInvoked { id, action } => {
                        trace!("Notification {id} actioned with {action}");
                        let pms = pms::MobileSettingsPanel::new().await?;
                        let _ = pms.open_panel("updates", None).await;
                    }
                }
            }

            () = &mut next_check => {
                if connectivity == nm::Connectivity::Full {
                    let _ = app.maybe_notify_update().await;
                    last_update_check = Some(Instant::now());
                }

                next_check = Box::pin(sleep(random_delay()));
            }

            _ = tokio::signal::ctrl_c() => {
                info!("Ctrl-C received, shutting down");
                break;
            }

        }
    }

    Ok(())
}
