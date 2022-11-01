use anyhow::{bail, Context, Result};
use clap::Parser;
use mimalloc::MiMalloc;
use std::collections::HashMap;
use tracing::debug;
use tracing_subscriber::EnvFilter;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

mod args;
mod monitor;
mod settings;

use crate::{args::Arguments, monitor::Monitor};

fn main() -> Result<()> {
    let arguments = Arguments::parse();
    init_log().context("Failed to initialize logging")?;
    debug!("Run with {:?}", arguments);

    if !arguments.config_file.exists() {
        bail!(
            "Configuration file '{}' does not exist.",
            arguments
                .config_file
                .to_str()
                .expect("path of configuration file")
        );
    }

    let settings = settings::load_config(Some(arguments.config_file))?;
    debug!(
        "Run with configuration {:?}",
        settings
            .build_cloned()?
            .try_deserialize::<HashMap<String, String>>()
            .unwrap()
    );

    Monitor::new(settings)?.listen()
}

fn init_log() -> Result<()> {
    let filter = match EnvFilter::try_from_env("RUST_LOG") {
        Ok(f) => f,
        Err(_) => EnvFilter::try_new("xhci_hcd_rebind=info")?,
    };
    if let Err(err) = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .without_time()
        .try_init()
    {
        bail!("Failed to initialize tracing subscriber: {err}");
    }

    Ok(())
}
