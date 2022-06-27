use std::fs;
use std::io::{BufReader, Read};
use serde::{Deserialize};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub device_name: String,
    pub device_addr: String,
    pub api_port: u16,
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

    pub fn from_file(path: &str) -> Config {
        let s = match read_file(path.to_owned()) {
            Ok(s) => s,
            Err(e) => panic!("fail to read config file: {}", e),
        };
        let config: Result<Config, toml::de::Error> = toml::from_str(&s);
        match config {
            Ok(c) => return c,
            Err(e) => panic!("fail to parse {}: {}", path, e),
        };
    }

}
