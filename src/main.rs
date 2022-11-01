use crate::{args::Arguments, monitor::Monitor};
use anyhow::{bail, Result};
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

fn main() -> Result<()> {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or(EnvFilter::try_new("xhci_hcd_rebind=info")?);
    if let Err(err) = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .without_time()
        .try_init()
    {
        bail!("Failed to initialize tracing subscriber: {err}");
    }

    let arguments = Arguments::parse();
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
