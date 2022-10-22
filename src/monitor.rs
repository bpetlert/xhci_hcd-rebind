use anyhow::{bail, Result};
use config::{builder::DefaultState, ConfigBuilder};
use std::{fs, io::Write, process::Command, time::Duration};
use systemd::{daemon, journal, Journal};
use tracing::{debug, info, warn};
use wait_timeout::ChildExt;

const BIND_BUS_FILE: &str = "/sys/bus/pci/drivers/xhci_hcd/bind";
const UNBIND_BUS_FILE: &str = "/sys/bus/pci/drivers/xhci_hcd/unbind";
const SCRIPT_TIMEOUT: u64 = 20; // seconds

pub struct Monitor {
    bus_id: String,
    bus_rebind_delay: u64,
    next_fail_check_delay: u64,
    pre_unbind_script: String,
    post_rebind_script: String,
}

impl Monitor {
    pub fn new(settings: ConfigBuilder<DefaultState>) -> Result<Self> {
        let settings = settings.build()?;
        Ok(Self {
            bus_id: settings.get::<String>("bus-id")?,
            bus_rebind_delay: settings.get::<u64>("bus-rebind-delay")?,
            next_fail_check_delay: settings.get::<u64>("next-fail-check-delay")?,
            pre_unbind_script: settings.get::<String>("pre-unbind-script")?,
            post_rebind_script: settings.get::<String>("post-rebind-script")?,
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
            if let Some(entry) = journal.await_next_entry(None)? {
                if let Some(log_msg) = entry.get("MESSAGE") {
                    if !self.is_fail(log_msg)? {
                        continue;
                    }
                    info!("xhci_hcd failed, {log_msg}");

                    // Run pre unbind command
                    if !self.pre_unbind_script.is_empty() {
                        info!("Run pre unbind command {}", self.pre_unbind_script);
                        if let Err(err) = self.run_script(&self.pre_unbind_script, SCRIPT_TIMEOUT) {
                            warn!("Failed to execute {}, {err}", self.pre_unbind_script);
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
                    if !self.post_rebind_script.is_empty() {
                        info!("Run post rebind command {}", self.post_rebind_script);
                        if let Err(err) = self.run_script(&self.post_rebind_script, SCRIPT_TIMEOUT)
                        {
                            warn!("Failed to execute {}, {err}", self.post_rebind_script);
                        }
                    }

                    // Delay for next bus failure checking
                    info!(
                        "Delay {} seconds for next bus failure checking...",
                        self.next_fail_check_delay
                    );
                    std::thread::sleep(std::time::Duration::from_secs(self.next_fail_check_delay));
                    info!("Wait for next xhci_hcd error...");
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

    fn run_script(&self, path: &str, timeout: u64) -> Result<()> {
        let mut script = match Command::new(path).spawn() {
            Ok(script) => script,
            Err(err) => bail!("Failed to execute {path}, {err}"),
        };

        match script.wait_timeout(Duration::from_secs(timeout))? {
            Some(exit_code) => {
                info!("Finished {path}, {exit_code}");
                Ok(())
            }
            None => {
                script.kill()?;
                script.wait()?;
                bail!("Execute timeout {path}, >= {timeout} seconds")
            }
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
