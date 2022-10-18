use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Arguments {
    /// Bus ID
    #[arg(long)]
    pub bus_id: Option<String>,

    /// Bus rebind delay (seconds)
    #[arg(long, default_value = "3")]
    pub bus_rebind_delay: u64,

    /// Next bus fail check delay (seconds)
    #[arg(long, default_value = "300")]
    pub next_fail_check_delay: u64,
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
        assert_eq!(args.bus_id, None);
        assert_eq!(args.bus_rebind_delay, 3);
        assert_eq!(args.next_fail_check_delay, 300);

        // Full long arguments
        let args = Arguments::from_arg_matches(&Arguments::command().get_matches_from(vec![
            env!("CARGO_CRATE_NAME"),
            "--bus-id",
            "0000:05:00.0",
            "--bus-rebind-delay",
            "5",
            "--next-fail-check-delay",
            "500",
        ]))
        .expect("Paring argument");
        assert_eq!(args.bus_id, Some("0000:05:00.0".to_string()));
        assert_eq!(args.bus_rebind_delay, 5);
        assert_eq!(args.next_fail_check_delay, 500);
    }
}
