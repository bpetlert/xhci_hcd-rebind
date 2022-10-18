use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use mimalloc::MiMalloc;
use tracing::debug;
use tracing_subscriber::EnvFilter;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

mod args;
mod monitor;

use crate::{args::Arguments, monitor::Monitor};

fn main() -> anyhow::Result<()> {
    let arguments = Arguments::parse();
    init_log().context("Failed to initialize logging")?;
    debug!("Run with {:?}", arguments);

    if let Some(bus_id) = arguments.bus_id {
        Monitor::new(
            &bus_id,
            arguments.bus_rebind_delay,
            arguments.next_fail_check_delay,
        )
        .listen()?;
    } else {
        return Err(anyhow!("--bus-id <BUS_ID> was missing"));
    }

    Ok(())
}

fn init_log() -> Result<()> {
    let filter = match EnvFilter::try_from_env("RUST_LOG") {
        Ok(f) => f,
        Err(_) => EnvFilter::try_new("xhci_hcd_rebind=warn")?,
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
