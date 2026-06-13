mod update_info;
pub use update_info::{UpdateError, UpdateInfo};

#[cfg(feature = "systemd-dbus-sysupdate1")]
mod sysupdate1;

#[cfg(feature = "systemd-dbus-sysupdate1")]
pub use sysupdate1::Sysupdate1UpdateChecker as UpdateChecker;
