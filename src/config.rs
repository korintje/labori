use std::fs;
use std::io::{BufReader, Read};
use serde::{Deserialize};

const CONFIG_FILENAME: &str = "config.toml";


#[derive(Debug, Deserialize)]
struct Config {
    device_name: String,
    device_addr: String,
    socket_port: u16,
    measurement_method: String,
    measurement_interval_sec: f32,
    sampling_time_millisec: i32,
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


fn get_config() -> Config {
    let path = CONFIG_FILENAME;
    let s = match read_file(path.to_owned()) {
        Ok(s) => s,
        Err(e) => panic!("fail to read config file: {}", e),
    };
    let config: Result<Config, toml::de::Error> = toml::from_str(&s);
    match config {
        Ok(c) => return c,
        Err(e) => panic!("fail to parse {}: {}", CONFIG_FILENAME, e),
    };
}


pub fn get_socket_port() -> u16 {
    get_config().socket_port
}