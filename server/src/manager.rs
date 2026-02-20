use std::collections::HashMap;

use futures_util::stream::StreamExt;
use syncthing::{Error, Syncthing, Update};
use tokio::task::JoinHandle;
use tokio::time::{Duration, sleep};
use url::Url;
use zbus::connection::Connection;
use zbus::interface;

use crate::config::get_syncthing_configuration;
use crate::folder::Folder;
use crate::{syncthing, systemd};

const BUS_NAME: &str = "mobi.phosh.syncbus";
const MANAGER_PATH: &str = "/mobi/phosh/syncbus/manager";
const FOLDERS_PATH: &str = "/mobi/phosh/syncbus/folders";

const SYNCTHING_SYSTEMD_UNIT: &str = "syncthing.service";

const MAX_RETRIES: u32 = 8;

pub struct Manager {
    enabled: bool,
    error: Option<String>,
    url: Url,

    cnx: Connection,
    folders: HashMap<String, usize>,
    handle: Option<JoinHandle<()>>,
    syncthing: Syncthing,
    systemd: systemd::SystemdProxy<'static>,
}

#[interface(name = "mobi.phosh.syncbus.Manager")]
impl Manager {
    #[zbus(property)]
    fn enabled(&self) -> bool {
        self.enabled
    }

    #[zbus(property)]
    fn error(&self) -> &str {
        self.error.as_deref().unwrap_or("")
    }

    #[zbus(property)]
    fn url(&self) -> String {
        self.url.to_string()
    }

    async fn start(&self) -> zbus::fdo::Result<()> {
        self.systemd
            .start_unit(SYNCTHING_SYSTEMD_UNIT, systemd::Mode::Replace)
            .await?;
        Ok(())
    }

    async fn stop(&self) -> zbus::fdo::Result<()> {
        self.systemd
            .stop_unit(SYNCTHING_SYSTEMD_UNIT, systemd::Mode::Replace)
            .await?;
        Ok(())
    }
}

impl Manager {
    #[allow(clippy::missing_errors_doc)]
    pub async fn new() -> zbus::Result<Self> {
        let config = match get_syncthing_configuration().await {
            Ok(config) => config,
            Err(error) => return Err(zbus::Error::Failure(error.to_string())),
        };

        let cnx = Connection::session().await?;
        let syncthing = Syncthing::new(&config.api_key, &config.url);
        let systemd = systemd::SystemdProxy::new(&cnx).await?;

        let manager = Manager {
            enabled: false,
            error: None,
            url: config.url,
            cnx,
            folders: HashMap::new(),
            handle: None,
            syncthing,
            systemd,
        };

        Ok(manager)
    }

    #[allow(clippy::missing_errors_doc)]
    pub async fn serve(self) -> zbus::Result<()> {
        let cnx = self.cnx.clone();
        let systemd = self.systemd.clone();

        let server = cnx.object_server();

        tracing::info!("Serving manager: {MANAGER_PATH}");
        server.at(MANAGER_PATH, self).await?;

        tracing::info!("Serving object manager: {FOLDERS_PATH}");
        let manager = zbus::fdo::ObjectManager {};
        server.at(FOLDERS_PATH, manager).await?;

        tracing::info!("Requesting bus name: {BUS_NAME}");
        cnx.request_name(BUS_NAME).await?;

        tokio::spawn(async move {
            if let Err(e) = watch_active_state(cnx, systemd).await {
                tracing::error!("Unable to watch active_state: {e}");
            }
        });

        Ok(())
    }
}

async fn set_url(cnx: &Connection, url: Url) -> zbus::Result<()> {
    tracing::debug!("Setting url: {url:#?}");
    let server = cnx.object_server();
    let iface_ref = server.interface::<_, Manager>(MANAGER_PATH).await?;
    let mut iface = iface_ref.get_mut().await;
    iface.url = url;
    iface.url_changed(iface_ref.signal_emitter()).await?;
    Ok(())
}

async fn set_error(cnx: &Connection, error: Option<String>) -> zbus::Result<()> {
    tracing::debug!("Setting error: {error:#?}");
    let server = cnx.object_server();
    let iface_ref = server.interface::<_, Manager>(MANAGER_PATH).await?;
    let mut iface = iface_ref.get_mut().await;
    iface.error = error;
    iface.error_changed(iface_ref.signal_emitter()).await?;
    Ok(())
}

async fn set_folder(
    cnx: &Connection,
    id: String,
    completion: Option<u8>,
    state: Option<String>,
    paused: Option<bool>,
) -> zbus::Result<()> {
    tracing::debug!("Setting folder: {id:#?} {completion:#?} {state:#?} {paused:#?}");
    let server = cnx.object_server();
    let iface_ref = server.interface::<_, Manager>(MANAGER_PATH).await?;
    let iface = iface_ref.get_mut().await;
    let idx = iface.folders.get(&id).ok_or(zbus::Error::Failure(format!(
        "Unable to get folder for {id}"
    )))?;
    let folder_ref = server
        .interface::<_, Folder>(format!("{FOLDERS_PATH}/{idx}"))
        .await?;
    let mut folder = folder_ref.get_mut().await;

    if let Some(completion) = completion {
        folder.completion = completion;
        folder
            .completion_changed(folder_ref.signal_emitter())
            .await?;
    }

    if let Some(state) = state {
        folder.state = state;
        folder.state_changed(folder_ref.signal_emitter()).await?;
    }

    if let Some(paused) = paused {
        folder.paused = paused;
        folder.paused_changed(folder_ref.signal_emitter()).await?;
    }

    Ok(())
}

async fn set_folders(cnx: &Connection, folders: Vec<Folder>) -> zbus::Result<()> {
    tracing::debug!("Setting folders: {}", folders.len());
    let server = cnx.object_server();
    let iface_ref = server.interface::<_, Manager>(MANAGER_PATH).await?;
    let mut iface = iface_ref.get_mut().await;

    for (_, idx) in iface.folders.drain() {
        tracing::debug!("Removing folder at {}", idx);
        server
            .remove::<Folder, _>(format!("{FOLDERS_PATH}/{idx}"))
            .await?;
    }

    for (idx, folder) in folders.into_iter().enumerate() {
        tracing::debug!(
            "Serving folder: {} ({}) at {}",
            folder.label,
            folder.id,
            idx
        );
        let id = folder.id.clone();
        server.at(format!("{FOLDERS_PATH}/{idx}"), folder).await?;
        iface.folders.insert(id, idx);
    }

    Ok(())
}

async fn watch_updates(cnx: Connection, syncthing: Syncthing) -> zbus::Result<()> {
    let mut bootstrap = true;
    let mut retries = MAX_RETRIES;
    let mut duration = Duration::new(1, 0);

    loop {
        let updates = match syncthing.poll(bootstrap).await {
            Ok(updates) => {
                set_error(&cnx, None).await?;
                bootstrap = false;
                retries = MAX_RETRIES;
                duration = Duration::new(1, 0);
                updates
            }
            Err(error) => {
                let details = error.to_string();
                tracing::error!("Failed to poll Syncthing: {details}");
                set_error(&cnx, Some(details)).await?;

                match error {
                    Error::Authorization(_) | Error::Connection(_) => {
                        assert!(retries != 0);
                        tracing::info!("Will retry in {duration:#?}");
                        sleep(duration).await;
                        retries -= 1;
                        duration *= 2;
                        continue;
                    }
                    Error::Deserialization(_) | Error::Unknown(_) => {
                        panic!();
                    }
                }
            }
        };

        for update in updates {
            match update {
                Update::Folders(folders) => set_folders(&cnx, folders).await?,
                Update::Folder {
                    id,
                    completion,
                    state,
                    paused,
                } => set_folder(&cnx, id, completion, state, paused).await?,
                Update::Gui { url } => set_url(&cnx, url).await?,
                Update::None => {}
            }
        }
    }
}

async fn set_enabled(cnx: &Connection, enabled: bool) -> zbus::Result<()> {
    tracing::debug!("Setting enabled: {enabled}");
    let server = cnx.object_server();
    let iface_ref = server.interface::<_, Manager>(MANAGER_PATH).await?;

    let handle;
    let syncthing;
    {
        let mut iface = iface_ref.get_mut().await;
        iface.enabled = enabled;
        iface.enabled_changed(iface_ref.signal_emitter()).await?;
        handle = iface.handle.take();
        syncthing = iface.syncthing.clone();
    }

    if let Some(handle) = handle {
        tracing::debug!("Aborting existing watch_updates");
        handle.abort();
        _ = handle.await;
        set_error(cnx, None).await?;
    }

    if enabled {
        let cnx = cnx.clone();
        let handle = tokio::spawn(async move {
            if let Err(e) = watch_updates(cnx, syncthing).await {
                tracing::error!("Unable to watch updates: {e}");
            }
        });

        {
            let mut iface = iface_ref.get_mut().await;
            iface.handle = Some(handle);
        }
    }

    Ok(())
}

async fn watch_active_state(
    cnx: Connection,
    systemd: systemd::SystemdProxy<'_>,
) -> zbus::Result<()> {
    let unit = systemd.load_unit(SYNCTHING_SYSTEMD_UNIT).await?;

    let mut stream = unit.receive_active_state_changed().await;
    while let Some(value) = stream.next().await {
        let active_state = value.get().await?;
        tracing::info!("Syncthing active_state changed: {active_state:#?}");

        if active_state == systemd::ActiveState::Active {
            set_enabled(&cnx, true).await?;
        } else if active_state == systemd::ActiveState::Inactive
            || active_state == systemd::ActiveState::Failed
        {
            set_enabled(&cnx, false).await?;
            set_folders(&cnx, Vec::new()).await?;
        }
    }

    Ok(())
}
