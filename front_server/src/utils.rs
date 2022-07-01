use std::fs;
use std::io::{BufReader, Read};
use serde::{Deserialize};
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
struct Config {
  web_url: Option<String>,
  db_path: Option<String>,
}

// Ref: https://cipepser.hatenablog.com/entry/rust-toml
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
  let path = "./reservoir.toml";
  let s = match read_file(path.to_owned()) {
    Ok(s) => s,
    Err(e) => panic!("fail to read file: {}", e),
  };
  let config: Result<Config, toml::de::Error> = toml::from_str(&s);
  match config {
    Ok(c) => return c,
    Err(e) => panic!("fail to parse toml: {}", e),
  };
}

pub fn get_url() -> String {
  get_config()
    .web_url
    .unwrap_or("127.0.0.1:8080".to_string())
}

pub fn get_db_path() -> String {
  get_config()
    .db_path
    .unwrap_or("./db/reservoir.db".to_string())
}

pub fn get_dev_names() -> HashMap<u16, String> {
  HashMap::from([(50001, "Iwatsu".to_string())])
}