use std::path::{Path, PathBuf};

use quick_xml::de::from_str;
use serde::Deserialize;
use url::Url;

#[derive(Debug, Deserialize)]
struct Configuration {
    gui: Gui,
}

#[derive(Debug, Deserialize)]
struct Gui {
    address: String,
    apikey: String,
}

pub struct Config {
    pub api_key: String,
    pub url: Url,
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

async fn get_config_path() -> Option<PathBuf> {
    if let Ok(conf_dir) = std::env::var("STCONFDIR") {
        let mut path = PathBuf::from(conf_dir);
        path.push("syncthing");
        path.push("config.xml");
        return Some(path);
    }

    let mut path = if let Ok(config_dir) = std::env::var("XDG_CONFIG_HOME") {
        PathBuf::from(config_dir)
    } else {
        let mut path = std::env::home_dir().unwrap_or_default();
        path.push(".config");
        path
    };
    path.push("syncthing");
    path.push("config.xml");

    if tokio::fs::try_exists(&path).await.unwrap_or_default() {
        return Some(path);
    }

    let mut path = if let Ok(state_dir) = std::env::var("XDG_STATE_HOME") {
        PathBuf::from(state_dir)
    } else {
        let mut path = std::env::home_dir().unwrap_or_default();
        path.push(".local");
        path.push("state");
        path
    };
    path.push("syncthing");
    path.push("config.xml");

    if tokio::fs::try_exists(&path).await.unwrap_or_default() {
        return Some(path);
    }

    None
}

pub fn address_to_url(address: &str) -> Result<Url> {
    if Path::new(address).is_absolute() {
        return Err(format!("{address} is not a supported URL").into());
    }

    let normalized = if address.starts_with(':') {
        format!("127.0.0.1{address}")
    } else {
        address.to_string()
    };

    let full_url = format!("http://{normalized}");
    let url = Url::parse(&full_url)?;

    Ok(url)
}

pub async fn get_syncthing_configuration() -> Result<Config> {
    let Some(path) = get_config_path().await else {
        let error = std::io::Error::new(
            std::io::ErrorKind::NotFound,
            String::from("unable to locate Syncthing configuration"),
        );
        return Err(error.into());
    };
    tracing::debug!("Reading configuration from {}", path.display());
    let xml = tokio::fs::read_to_string(path).await?;
    let config: Configuration = from_str(&xml)?;
    let api_key = config.gui.apikey;
    let url = address_to_url(&config.gui.address)?;

    Ok(Config { api_key, url })
}
