// SPDX-FileCopyrightText: 2026 Phosh.mobi e.V.
// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::{Duration, Instant};

use clap::Parser;
use gettextrs::{bind_textdomain_codeset, bindtextdomain, gettext, setlocale, textdomain};
use log::{debug, info, trace, warn};
use phosh_os_updater::config;
use phosh_os_updater::updater::{UpdateChecker, UpdateError, UpdateInfo};
use phosh_session_services::{nm, noti, pms};
use tokio::time::sleep;

#[derive(Debug, Clone, Copy)]
struct ConnectionState {
    connectivity: nm::Connectivity,
    metered: nm::Metered,
}

impl Default for ConnectionState {
    fn default() -> Self {
        Self {
            connectivity: nm::Connectivity::Unknown,
            metered: nm::Metered::Unknown,
        }
    }
}

impl ConnectionState {
    fn is_connected(self) -> bool {
        self.connectivity == nm::Connectivity::Full
    }

    fn is_metered(self) -> bool {
        matches!(
            self.metered,
            nm::Metered::Yes | nm::Metered::GuessYes | nm::Metered::Unknown
        )
    }

    fn can_update(self) -> bool {
        self.is_connected() && !self.is_metered()
    }

    // Update connection state. Returns true if the state changed to a state we want to pull
    // updates over.
    fn update(&mut self, new_nm_state: (nm::Connectivity, nm::Metered)) -> bool {
        let old = *self;
        (self.connectivity, self.metered) = new_nm_state;

        self.can_update() && (old.connectivity != self.connectivity || old.metered != self.metered)
    }
}

struct Service {
    nm: nm::NetworkManager,
    noti_manager: noti::NotiManager,
    update_checker: UpdateChecker,
    noti_id: Option<u32>,
    update: Option<UpdateInfo>,
    conn_state: ConnectionState,
}

impl Service {
    pub async fn new(app_id: &str) -> zbus::Result<Self> {
        let network_manager = nm::NetworkManager::new().await?;

        Ok(Self {
            nm: network_manager,
            noti_manager: noti::NotiManager::new(app_id).await?,
            update_checker: UpdateChecker::new().await?,
            noti_id: None,
            update: None,
            conn_state: ConnectionState::default(),
        })
    }

    fn start_nm_monitor(&self) -> async_channel::Receiver<(nm::Connectivity, nm::Metered)> {
        let (tx, rx) = async_channel::bounded(1);

        let nm = self.nm.clone();
        tokio::spawn(async move {
            nm.monitor_connectivity(move |c, m| {
                if let Err(err) = tx.force_send((c, m)) {
                    trace!("Failed to send connectivity update: {err}");
                }
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
                if let Err(err) = tx.try_send(c) {
                    trace!("Failed to send notification signal: {err}");
                }
            })
            .await
            .unwrap();
        });

        rx
    }

    async fn notify_update(&mut self) -> zbus::Result<()> {
        let update = self.update.as_ref().unwrap();
        let msg = gettext("Update to {} available").replace("{}", &update.version);
        let noti = noti::Noti::new()
            .summary(gettext("OS Update available"))
            .actions(vec!["install.update".into(), gettext("Install Update")])
            .body(&msg);
        let id = self.noti_manager.notify(noti, None, self.noti_id).await?;

        trace!("Notification {} for {} sent", id, update.version);
        self.noti_id = Some(id);

        Ok(())
    }

    async fn maybe_notify_update(&mut self) -> std::result::Result<(), UpdateError> {
        if !self.conn_state.can_update() {
            trace!("No connection we should update over");
            return Ok(());
        }

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

            // Bubble up NotSupported so we can quit
            Err(e @ UpdateError::NotSupported(_)) => return Err(e),

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

    Duration::from_hours(hours)
}

fn i18n_init() {
    unsafe {
        setlocale(gettextrs::LocaleCategory::LcAll, "");
    }
    bindtextdomain(config::GETTEXT_PACKAGE, config::LOCALEDIR)
        .expect("Unable to bind the text domain");
    bind_textdomain_codeset(config::GETTEXT_PACKAGE, "UTF-8")
        .expect("Unable to set the text domain encoding");
    textdomain(config::GETTEXT_PACKAGE).expect("Unable to switch to the text domain");
}

#[tokio::main]
async fn main() -> zbus::Result<()> {
    tracing_subscriber::fmt::init();

    i18n_init();

    let _args = Args::parse();
    let mut app = Service::new("mobi.phosh.OsUpdater").await?;
    let nm_rx = app.start_nm_monitor();
    let noti_rx = app.start_noti_monitor();
    let mut next_check = Box::pin(sleep(random_delay()));
    let mut last_update_check = None;

    trace!("Entering listening loop…");
    loop {
        tokio::select! {
            Ok(new_nm_state) = nm_rx.recv() => {
                let can_check = app.conn_state.update(new_nm_state);

                if can_check && should_check_updates(last_update_check) {
                    match app.maybe_notify_update().await {
                        Err(UpdateError::NotSupported(_)) => {
                            info!("Update service not supported, quitting");
                            break;
                        },
                        Err(err) => {
                            debug!("Update check failed: {err}");
                        }
                        Ok(()) => {}
                    }

                    last_update_check = Some(Instant::now());
                    next_check = Box::pin(sleep(random_delay()));
                }
            }

            Ok(event) = noti_rx.recv() => {
                match event {
                    noti::Event::ActionInvoked { id, action } => {
                        trace!("Notification {id} actioned with {action}");
                        let pms = pms::MobileSettingsPanel::new().await?;
                        if let Err(err) = pms.open_panel("updates", None).await {
                            warn!("Panel open failed: {err}");
                        }
                    }
                }
            }

            () = &mut next_check => {
                if let Err(err) = app.maybe_notify_update().await {
                    warn!("Update check failed: {err}");
                }
                last_update_check = Some(Instant::now());
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

#[cfg(test)]
mod tests {
    use nm::{Connectivity, Metered};

    use super::*;

    fn state(connectivity: Connectivity, metered: Metered) -> ConnectionState {
        ConnectionState {
            connectivity,
            metered,
        }
    }

    #[test]
    fn default_cannot_update() {
        assert!(!ConnectionState::default().can_update());
    }

    #[test]
    fn can_update_only_on_full_unmetered() {
        for (metered, expected) in [
            (Metered::No, true),
            (Metered::GuessNo, true),
            (Metered::Yes, false),
            (Metered::GuessYes, false),
            (Metered::Unknown, false),
        ] {
            assert_eq!(
                state(Connectivity::Full, metered).can_update(),
                expected,
                "Full/{metered:?}"
            );
        }
    }

    #[test]
    fn cannot_update_without_full_connectivity() {
        for connectivity in [
            Connectivity::Unknown,
            Connectivity::None,
            Connectivity::Portal,
            Connectivity::Limited,
        ] {
            assert!(
                !state(connectivity, Metered::No).can_update(),
                "{connectivity:?}/No"
            );
        }
    }

    #[test]
    fn update_reports_transition_to_usable() {
        let mut s = ConnectionState::default();
        assert!(s.update((Connectivity::Full, Metered::No)));
        assert!(s.can_update());
    }

    #[test]
    fn update_no_check_while_metered() {
        let mut s = ConnectionState::default();
        assert!(!s.update((Connectivity::Full, Metered::Yes)));
        assert!(!s.update((Connectivity::Full, Metered::GuessYes)));
        assert!(!s.can_update());
    }

    #[test]
    fn update_metered_to_unmetered_triggers() {
        let mut s = state(Connectivity::Full, Metered::Yes);
        assert!(s.update((Connectivity::Full, Metered::No)));
    }

    #[test]
    fn update_unmetered_to_metered_does_not_trigger() {
        let mut s = state(Connectivity::Full, Metered::No);
        assert!(!s.update((Connectivity::Full, Metered::Yes)));
        assert!(!s.can_update());
    }

    #[test]
    fn update_reconnect_triggers() {
        let mut s = state(Connectivity::Full, Metered::No);
        assert!(!s.update((Connectivity::None, Metered::No)));
        assert!(s.update((Connectivity::Full, Metered::No)));
    }

    #[test]
    fn update_same_state_does_not_retrigger() {
        let mut s = state(Connectivity::Full, Metered::No);
        assert!(!s.update((Connectivity::Full, Metered::No)));
    }

    #[test]
    fn update_between_unmetered_guesses_triggers() {
        let mut s = state(Connectivity::Full, Metered::GuessNo);
        assert!(s.update((Connectivity::Full, Metered::No)));
    }
}
