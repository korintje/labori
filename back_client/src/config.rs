use std::fs;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use serde::Deserialize;
use crate::error::LaboriError;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub device_name: String,
    pub device_addr: String,
    pub api_port: u16,
    pub database_path: String,
    pub gpio_settle_millis: u64,
}

fn read_file(path: String) -> Result<String, String> {
    let mut file_content = String::new();
    let mut fr = fs::File::open(path)
        .map(|f| BufReader::new(f))
        .map_err(|e| e.to_string())?;
    fr.read_to_string(&mut file_content)
        .map_err(|e| e.to_string())?;
    Ok(file_content)
}

impl Config {

    pub fn from_file(path: &str) -> Result<Config, LaboriError> {
        let s = read_file(path.to_owned())
            .map_err(LaboriError::ConfigError)?;
        let config: Result<Config, toml::de::Error> = toml::from_str(&s);
        match config {
            Ok(mut c) => {
                let config_path = PathBuf::from(path);
                let database_path = PathBuf::from(&c.database_path);
                if database_path.is_relative() {
                    let parent = config_path.parent().unwrap_or_else(|| Path::new("."));
                    c.database_path = parent.join(database_path)
                        .to_string_lossy()
                        .into_owned();
                }
                Ok(c)
            },
            Err(e) => Err(LaboriError::ConfigError(
                format!("fail to parse {}: {}", path, e)
            )),
        }
    }

}
