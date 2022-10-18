use anyhow::bail;
use std::{fs, io::Write};
use systemd::{daemon, journal, Journal};
use tracing::{debug, info, warn};

const BIND_BUS_FILE: &str = "/sys/bus/pci/drivers/xhci_hcd/bind";
const UNBIND_BUS_FILE: &str = "/sys/bus/pci/drivers/xhci_hcd/unbind";

pub struct Monitor {
    bus_id: String,
    bus_rebind_delay: u64,
    next_fail_check_delay: u64,
}

impl Monitor {
    pub fn new(bus_id: &str, bus_rebind_delay: u64, next_fail_check_delay: u64) -> Self {
        Self {
            bus_id: bus_id.to_string(),
            bus_rebind_delay,
            next_fail_check_delay,
        }
    }

    pub fn listen(self) -> anyhow::Result<()> {
        let mut journal: Journal = journal::OpenOptions::default()
            .system(true)
            .local_only(true)
            .runtime_only(false)
            .all_namespaces(true)
            .open()?;

        // Go to end of journal
        journal.seek_tail()?;
        while journal.next_skip(1)? > 0 {}

        // Filter
        journal.match_add("_BOOT_ID", "1")?; // Only current boot log message
        journal.match_add("_TRANSPORT", "kernel")?; // Only kernel message
        journal.match_add("PRIORITY", "4")?; // Only warning message

        debug!("Notify systemd that we are ready :)");
        if !daemon::notify(false, vec![("READY", "1")].iter())? {
            bail!("Cannot notify systemd, READY=1");
        }

        let notify_msg = format!("Start monitor xhci_hcd failure on bus {}", self.bus_id);
        debug!(notify_msg);
        if !daemon::notify(false, vec![("STATUS", &notify_msg)].iter())? {
            bail!("Cannot notify systemd, STATUS={notify_msg}");
        }

        loop {
            if let Some(entry) = journal.await_next_entry(None)? {
                if let Some(log_msg) = entry.get("MESSAGE") {
                    if self.is_fail(log_msg)? {
                        // Unbind bus
                        match fs::OpenOptions::new()
                            .write(true)
                            .append(true)
                            .open(UNBIND_BUS_FILE)
                        {
                            Ok(mut unbind_bus) => {
                                if let Err(err) = unbind_bus.write_all(self.bus_id.as_bytes()) {
                                    warn!("{err}");
                                    continue;
                                } else {
                                    info!("Unbind bus {}", self.bus_id);
                                }
                            }
                            Err(err) => {
                                warn!("{err}");
                                continue;
                            }
                        };

                        // Rebind bus
                        std::thread::sleep(std::time::Duration::from_secs(self.bus_rebind_delay));
                        match fs::OpenOptions::new()
                            .write(true)
                            .append(true)
                            .open(BIND_BUS_FILE)
                        {
                            Ok(mut bind_bus) => {
                                if let Err(err) = bind_bus.write_all(self.bus_id.as_bytes()) {
                                    warn!("{err}");
                                    continue;
                                } else {
                                    info!("Rebind bus {}", self.bus_id);
                                }
                            }
                            Err(err) => {
                                warn!("{err}");
                                continue;
                            }
                        }

                        info!("Successfully rebind bus {}", self.bus_id);

                        // Delay for next bus failure checking
                        info!(
                            "Delay {} seconds for next bus failure checking",
                            self.next_fail_check_delay
                        );
                        std::thread::sleep(std::time::Duration::from_secs(
                            self.next_fail_check_delay,
                        ));
                    }
                }
            }
        }
    }

    fn is_fail(&self, log_msg: &str) -> anyhow::Result<bool> {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xhci_hcd_fail_log() {
        let journal_log1 = "xhci_hcd 0000:04:00.0: WARN: TRB error for slot 1 ep 5 on endpoint";
        let journal_log2 = "xhci_hcd 0000:04:00.0: WARN waiting for error on ep to be cleared";

        let mon = Monitor::new("0000:04:00.0", 3, 300);
        assert!(!mon.is_fail(journal_log1).unwrap());
        assert!(mon.is_fail(journal_log2).unwrap());
    }
}
