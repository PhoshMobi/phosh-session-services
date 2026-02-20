use serde::{Deserialize, Serialize};
use zbus::proxy;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Type, Value};

#[derive(Deserialize, Serialize, Type, Value, OwnedValue, Clone, Debug, PartialEq)]
#[serde(rename_all = "kebab-case")]
#[zvariant(signature = "s", rename_all = "kebab-case")]
pub enum ActiveState {
    Active,
    Inactive,
    Failed,
    Activating,
    Deactivating,
    Maintenance,
    Reloading,
    Refreshing,
}

#[proxy(
    interface = "org.freedesktop.systemd1.Unit",
    default_service = "org.freedesktop.systemd1"
)]
pub trait Unit {
    #[zbus(property)]
    fn active_state(&self) -> zbus::Result<ActiveState>;
}

#[derive(Deserialize, Serialize, Type, Value, Clone)]
#[serde(rename_all = "kebab-case")]
#[zvariant(signature = "s", rename_all = "kebab-case")]
pub enum Mode {
    Replace,
    Fail,
    Isolate,
    IgnoreDependencies,
    IgnoreRequirements,
}

#[proxy(
    interface = "org.freedesktop.systemd1.Manager",
    default_service = "org.freedesktop.systemd1",
    default_path = "/org/freedesktop/systemd1"
)]
pub trait Systemd {
    #[zbus(object = "Unit")]
    fn load_unit(&self, name: &str);

    fn start_unit(&self, name: &str, mode: Mode) -> zbus::Result<OwnedObjectPath>;

    fn stop_unit(&self, name: &str, mode: Mode) -> zbus::Result<OwnedObjectPath>;

    fn subscribe(&self) -> zbus::Result<()>;
}
