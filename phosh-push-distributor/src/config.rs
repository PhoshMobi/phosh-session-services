// SPDX-FileCopyrightText: 2026 Phosh.mobi e.V.
// SPDX-License-Identifier: GPL-3.0-or-later

//! This only says *which* providers to use and where they
//! live. Secrets don't go in here, they're in the keyring, so unknown
//! keys are rejected.

use std::path::{Path, PathBuf};

use log::warn;
use serde::Deserialize;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const DIR_NAME: &str = "phosh-push-distributor";

#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Provider used for new registrations. Defaults to the first one
    /// configured.
    pub default_provider: Option<String>,
    pub ntfy: Option<NtfyConfig>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NtfyConfig {
    #[serde(default = "default_ntfy_server")]
    pub server: String,
}

fn default_ntfy_server() -> String {
    "https://ntfy.sh".to_string()
}

impl Config {
    /// # Errors
    ///
    /// Fails on syntax errors or unknown keys.
    pub fn parse(text: &str) -> Result<Self> {
        Ok(toml::from_str(text)?)
    }

    /// Load the configuration. A missing file is not an error: the service
    /// then runs without providers and tells connectors that action is
    /// required.
    ///
    /// # Errors
    ///
    /// Fails if the file exists but can't be read or parsed.
    pub async fn load(path: &Path) -> Result<Self> {
        match tokio::fs::read_to_string(path).await {
            Ok(text) => {
                Self::parse(&text).map_err(|error| format!("{}: {error}", path.display()).into())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                warn!(
                    "No configuration at {}, no providers enabled",
                    path.display()
                );
                Ok(Config::default())
            }
            Err(error) => Err(format!("{}: {error}", path.display()).into()),
        }
    }
}

fn xdg_dir(variable: &str, fallback: &[&str]) -> PathBuf {
    let mut path = if let Ok(dir) = std::env::var(variable) {
        PathBuf::from(dir)
    } else {
        let mut path = std::env::home_dir().unwrap_or_default();
        path.extend(fallback);
        path
    };
    path.push(DIR_NAME);
    path
}

#[must_use]
pub fn config_path() -> PathBuf {
    xdg_dir("XDG_CONFIG_HOME", &[".config"]).join("config.toml")
}

/// Where non-secret runtime state (resume cursors) lives.
#[must_use]
pub fn state_path() -> PathBuf {
    xdg_dir("XDG_STATE_HOME", &[".local", "state"]).join("cursors.json")
}
