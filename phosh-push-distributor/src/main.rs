// SPDX-FileCopyrightText: 2026 Phosh.mobi e.V.
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use clap::Parser;
use log::{error, info};
use phosh_push_distributor::bus::DbusBus;
use phosh_push_distributor::config::{self, Config};
use phosh_push_distributor::dbus::{self, DistributorInterface};
use phosh_push_distributor::distributor::Distributor;
use phosh_push_distributor::provider;
use phosh_push_distributor::registry::{Cursors, Registry};
use phosh_push_distributor::secrets::{Keyring, RegistrationStore};
use zbus::Connection;

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Configuration file to use
    #[arg(long, value_name = "FILE")]
    config: Option<PathBuf>,
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

async fn serve(
    config_path: Option<PathBuf>,
    registrations: Arc<dyn RegistrationStore>,
) -> Result<()> {
    let config = Config::load(&config_path.unwrap_or_else(config::config_path)).await?;
    let providers = provider::build(&config)?;
    let registry = Registry::load(registrations.as_ref()).await?;
    let cursors = Cursors::load(config::state_path()).await;

    let conn = Connection::session().await?;
    let (distributor, pushes) = Distributor::new(
        Arc::new(DbusBus::new(conn.clone())),
        providers,
        config.default_provider,
        registrations,
        registry,
        cursors,
    );
    distributor.resume().await;

    conn.object_server()
        .at(
            dbus::PATH,
            DistributorInterface::new(Arc::clone(&distributor)),
        )
        .await?;
    conn.request_name(dbus::BUS_NAME).await?;

    tokio::select! {
        () = distributor.run(pushes) => {}
        _ = tokio::signal::ctrl_c() => info!("Ctrl-C received, shutting down"),
    }
    Ok(())
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt::init();

    let args = Args::parse();
    let result = async {
        let registrations: Arc<dyn RegistrationStore> = Arc::new(Keyring::new().await?);
        serve(args.config, registrations).await
    }
    .await;

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            error!("{error}");
            ExitCode::FAILURE
        }
    }
}
