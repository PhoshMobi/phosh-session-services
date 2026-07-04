use std::future::pending;
use std::process::ExitCode;

use server::Manager;

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt::init();

    let manager = match Manager::new().await {
        Ok(manager) => manager,
        Err(error) => {
            tracing::error!("{error}");
            return ExitCode::FAILURE;
        }
    };

    if let Err(error) = manager.serve().await {
        tracing::error!("{error}");
        return ExitCode::FAILURE;
    }

    pending::<()>().await;

    ExitCode::SUCCESS
}
