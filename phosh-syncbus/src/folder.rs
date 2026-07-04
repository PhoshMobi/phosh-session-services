use zbus::interface;

use crate::syncthing::Syncthing;

pub struct Folder {
    pub id: String,
    pub label: String,
    pub path: String,
    pub completion: u8,
    pub state: String,
    pub paused: bool,
    syncthing: Syncthing,
}

#[interface(name = "mobi.phosh.syncbus.Folder")]
impl Folder {
    #[zbus(property(emits_changed_signal = "const"))]
    fn id(&self) -> &str {
        &self.id
    }

    #[zbus(property)]
    fn label(&self) -> &str {
        &self.label
    }

    #[zbus(property)]
    fn path(&self) -> &str {
        &self.path
    }

    #[zbus(property)]
    fn completion(&self) -> u8 {
        self.completion
    }

    #[zbus(property)]
    fn state(&self) -> &str {
        &self.state
    }

    #[zbus(property)]
    fn paused(&self) -> bool {
        self.paused
    }

    #[zbus(property)]
    async fn set_paused(&self, paused: bool) -> zbus::Result<()> {
        self.syncthing.set_paused(&self.id, paused).await?;
        Ok(())
    }
}

impl Folder {
    #[must_use]
    pub fn new(
        id: String,
        label: String,
        path: String,
        completion: u8,
        state: String,
        paused: bool,
        syncthing: Syncthing,
    ) -> Self {
        Folder {
            id,
            label,
            path,
            completion,
            state,
            paused,
            syncthing,
        }
    }
}
