use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Arguments {
    /// Configuration file
    #[arg(short, long, default_value = "/etc/systemd/xhci_hcd-rebind.toml")]
    pub config_file: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, FromArgMatches};

    #[test]
    fn test_args() {
        // Default arguments
        let args = Arguments::from_arg_matches(
            &Arguments::command().get_matches_from(vec![env!("CARGO_CRATE_NAME")]),
        )
        .expect("Paring argument");
        assert_eq!(
            args.config_file,
            PathBuf::from("/etc/systemd/xhci_hcd-rebind.toml")
        );

        // Full long arguments
        let args = Arguments::from_arg_matches(&Arguments::command().get_matches_from(vec![
            env!("CARGO_CRATE_NAME"),
            "--config-file",
            "/etc/systemd/xhci_hcd-rebind2.toml",
        ]))
        .expect("Paring argument");
        assert_eq!(
            args.config_file,
            PathBuf::from("/etc/systemd/xhci_hcd-rebind2.toml")
        );

        // Full short arguments
        let args = Arguments::from_arg_matches(&Arguments::command().get_matches_from(vec![
            env!("CARGO_CRATE_NAME"),
            "-c",
            "/etc/systemd/xhci_hcd-rebind3.toml",
        ]))
        .expect("Paring argument");
        assert_eq!(
            args.config_file,
            PathBuf::from("/etc/systemd/xhci_hcd-rebind3.toml")
        );
    }
}
