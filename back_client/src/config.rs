use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use serde::Deserialize;

use crate::error::{LaboriError, Result};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub device_addr: String,
    #[serde(default = "default_measurement_function")]
    pub measurement_function: String,
    pub listen_addr: String,
    pub database_path: String,
    pub web_root: String,
    #[serde(default = "default_gpio_settle_millis")]
    pub gpio_settle_millis: u64,
    #[serde(default = "default_storage_queue_capacity")]
    pub storage_queue_capacity: usize,
    #[serde(default = "default_storage_batch_size")]
    pub storage_batch_size: usize,
    #[serde(default = "default_storage_flush_millis")]
    pub storage_flush_millis: u64,
    #[serde(default = "default_reconnect_millis")]
    pub reconnect_millis: u64,
    #[serde(default = "default_instrument_timeout_millis")]
    pub instrument_timeout_millis: u64,
}

fn default_gpio_settle_millis() -> u64 {
    10
}
fn default_measurement_function() -> String {
    "FINA".to_string()
}
fn default_storage_queue_capacity() -> usize {
    100_000
}
fn default_storage_batch_size() -> usize {
    1_000
}
fn default_storage_flush_millis() -> u64 {
    100
}
fn default_reconnect_millis() -> u64 {
    500
}
fn default_instrument_timeout_millis() -> u64 {
    15_000
}

impl Config {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let source = fs::read_to_string(path)
            .map_err(|error| LaboriError::Config(format!("{}: {error}", path.display())))?;
        let mut config: Self = toml::from_str(&source)
            .map_err(|error| LaboriError::Config(format!("{}: {error}", path.display())))?;
        let base = path.parent().unwrap_or_else(|| Path::new("."));
        config.database_path = resolve(base, &config.database_path);
        config.web_root = resolve(base, &config.web_root);
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        const FUNCTIONS: [&str; 13] = [
            "FINA", "FINB", "FINC", "FLIN", "PER", "DUTY", "PWID", "TINT", "FRAT", "PHAS", "TOT",
            "VPPA", "VPPB",
        ];
        if !FUNCTIONS.contains(&self.measurement_function.as_str()) {
            return Err(LaboriError::Config(format!(
                "unsupported measurement_function: {}",
                self.measurement_function
            )));
        }
        if self.storage_queue_capacity == 0 || self.storage_batch_size == 0 {
            return Err(LaboriError::Config(
                "storage queue capacity and batch size must be greater than zero".into(),
            ));
        }
        if self.storage_batch_size > self.storage_queue_capacity {
            return Err(LaboriError::Config(
                "storage_batch_size must not exceed storage_queue_capacity".into(),
            ));
        }
        Ok(())
    }

    pub fn instrument_timeout(&self) -> Duration {
        Duration::from_millis(self.instrument_timeout_millis)
    }
}

fn resolve(base: &Path, value: &str) -> String {
    let path = PathBuf::from(value);
    let resolved = if path.is_relative() {
        base.join(path)
    } else {
        path
    };
    resolved.to_string_lossy().into_owned()
}
