use anyhow::{bail, Result};
use config::{builder::DefaultState, ConfigBuilder};
use duct::cmd;
use std::{
    fs,
    io::{BufRead, BufReader, Write},
};
use systemd::{daemon, journal, Journal};
use tracing::{debug, info, warn};

const BIND_BUS_FILE: &str = "/sys/bus/pci/drivers/xhci_hcd/bind";
const UNBIND_BUS_FILE: &str = "/sys/bus/pci/drivers/xhci_hcd/unbind";

pub struct Monitor {
    bus_id: String,
    bus_rebind_delay: u64,
    next_fail_check_delay: u64,
    pre_unbind_cmd: String,
    post_rebind_cmd: String,
}

impl Monitor {
    pub fn new(settings: ConfigBuilder<DefaultState>) -> Result<Self> {
        let settings = settings.build()?;
        Ok(Self {
            bus_id: settings.get::<String>("bus-id")?,
            bus_rebind_delay: settings.get::<u64>("bus-rebind-delay")?,
            next_fail_check_delay: settings.get::<u64>("next-fail-check-delay")?,
            pre_unbind_cmd: settings.get::<String>("pre-unbind-cmd")?,
            post_rebind_cmd: settings.get::<String>("post-rebind-cmd")?,
        })
    }

    pub fn listen(self) -> Result<()> {
        let mut journal: Journal = journal::OpenOptions::default()
            .system(true)
            .local_only(true)
            .runtime_only(false)
            .all_namespaces(true)
            .open()?;

        // Filter
        journal.match_add("_TRANSPORT", "kernel")?; // Only kernel message
        journal.match_add("PRIORITY", "4")?; // Only warning message

        // Go to end of journal
        journal.seek_tail()?;
        while journal.next_skip(1)? > 0 {}

        debug!("Notify systemd that we are ready :)");
        if !daemon::notify(false, vec![("READY", "1")].iter())? {
            bail!("Cannot notify systemd, READY=1");
        }

        let notify_msg = format!("Start monitor xhci_hcd failure on bus {}", self.bus_id);
        if !daemon::notify(false, vec![("STATUS", &notify_msg)].iter())? {
            bail!("Cannot notify systemd, STATUS={notify_msg}");
        }
        info!("{notify_msg}");
        loop {
            info!("Wait for xhci_hcd error...");
            if let Some(entry) = journal.await_next_entry(None)? {
                if let Some(log_msg) = entry.get("MESSAGE") {
                    if !self.is_fail(log_msg)? {
                        continue;
                    }
                    info!("xhci_hcd failed, {log_msg}");

                    // Run pre unbind command
                    if !self.pre_unbind_cmd.is_empty() {
                        info!("Run pre unbind command {}", self.pre_unbind_cmd);
                        if let Err(err) = self.run_cmd(&self.pre_unbind_cmd) {
                            warn!("Failed to execute {}, {err}", self.pre_unbind_cmd);
                        }
                    }

                    // Unbind bus
                    info!("Try to unbind bus {}", self.bus_id);
                    if let Err(err) = self.write_sysfs(UNBIND_BUS_FILE, &self.bus_id) {
                        warn!("{err}");
                        continue;
                    } else {
                        info!("Successfully unbind bus {}", self.bus_id);
                    }

                    // Rebind bus
                    std::thread::sleep(std::time::Duration::from_secs(self.bus_rebind_delay));
                    info!("Try to rebind bus {}", self.bus_id);
                    if let Err(err) = self.write_sysfs(BIND_BUS_FILE, &self.bus_id) {
                        warn!("{err}");
                        continue;
                    } else {
                        info!("Successfully rebind bus {}", self.bus_id);
                    }

                    // Run post rebind command
                    if !self.post_rebind_cmd.is_empty() {
                        info!("Run post rebind command {}", self.post_rebind_cmd);
                        if let Err(err) = self.run_cmd(&self.post_rebind_cmd) {
                            warn!("Failed to execute {}, {err}", self.post_rebind_cmd);
                        }
                    }

                    // Delay for next bus failure checking
                    info!(
                        "Delay {} seconds for next bus failure checking...",
                        self.next_fail_check_delay
                    );
                    std::thread::sleep(std::time::Duration::from_secs(self.next_fail_check_delay));
                }
            }
        }
    }

    fn is_fail(&self, log_msg: &str) -> Result<bool> {
        let fail_regex = {
            static RE: once_cell::sync::OnceCell<regex::Regex> = once_cell::sync::OnceCell::new();
            RE.get_or_init(|| {
                regex::Regex::new(
                    format!(
                        "xhci_hcd {}: WARN waiting for error on ep to be cleared",
                        self.bus_id.replace('.', r"\.")
                    )
                    .as_str(),
                )
                .expect("Creating xhci_hcd fail regex")
            })
        };

        Ok(fail_regex.is_match(log_msg))
    }

    fn run_cmd(&self, cmd: &str) -> Result<()> {
        match cmd!(cmd).stderr_to_stdout().reader() {
            Ok(reader) => {
                let lines = BufReader::new(reader)
                    .lines()
                    .filter_map(Result::ok)
                    .collect::<Vec<String>>()
                    .join("\n");
                info!("{}", lines);
                Ok(())
            }
            Err(err) => bail!("Failed to execute {cmd}, {err}"),
        }
    }

    fn write_sysfs(&self, path: &str, value: &str) -> Result<()> {
        match fs::OpenOptions::new().write(true).append(true).open(path) {
            Ok(mut fs) => {
                if let Err(err) = fs.write_all(value.as_bytes()) {
                    bail!("{err}");
                }
            }
            Err(err) => {
                bail!("{err}");
            }
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings;

    #[test]
    fn test_xhci_hcd_fail_log() {
        let journal_log1 = "xhci_hcd 0000:04:00.0: WARN: TRB error for slot 1 ep 5 on endpoint";
        let journal_log2 = "xhci_hcd 0000:04:00.0: WARN waiting for error on ep to be cleared";

        let settings = settings::load_config(None)
            .unwrap()
            .set_override("bus-id", "0000:04:00.0")
            .unwrap();
        let mon = Monitor::new(settings).unwrap();
        assert!(!mon.is_fail(journal_log1).unwrap());
        assert!(mon.is_fail(journal_log2).unwrap());
    }
}
