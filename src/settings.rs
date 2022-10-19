use anyhow::Result;
use config::{builder::DefaultState, Config, ConfigBuilder, FileFormat};
use std::path::PathBuf;

pub fn load_config(config_file: Option<PathBuf>) -> Result<ConfigBuilder<DefaultState>> {
    let mut settings = Config::builder()
        .set_default("bus-rebind-delay", 3)?
        .set_default("next-fail-check-delay", 300)?
        .set_default("pre-unbind-cmd", "")?
        .set_default("post-rebind-cmd", "")?;

    if let Some(mut config_file) = config_file {
        config_file.set_extension("");
        settings = settings.add_source(config::File::new(
            config_file.to_str().unwrap(),
            FileFormat::Toml,
        ));
    }

    Ok(settings)
}
