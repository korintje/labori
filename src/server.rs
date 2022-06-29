use std::net::{TcpListener, TcpStream};
use std::io::{BufReader, Write, Read, BufWriter};
use encoding::{Encoding, EncoderTrap, DecoderTrap};
use encoding::all::ASCII;
use tokio::sync::mpsc;
use crate::error::LaboriError;
use crate::config::Config;
use crate::model::{Command, Response};
use serde_json;

pub async fn serve(
    config: Config,
    tx: mpsc::Sender<Command>,
    rx: mpsc::Receiver<Response>,
) -> Result<(), LaboriError> {
    
    // let mut state = State::Holded;
    let listener = TcpListener::bind(format!("127.0.0.1:{}", config.api_port))?;
    loop {
        println!("Waiting for clients");
        let (stream, _addr) = listener.accept()?;
        println!("A Client has connected");
        let mut reader = BufReader::new(&stream);
        let mut writer = BufWriter::new(&stream);
        let mut buff = vec![0; 1024];
        let n = reader.read(&mut buff).expect("API RECEIVE FAILURE!!!");
        let request = ASCII.decode(&buff[0..n], DecoderTrap::Replace).unwrap();
        let command: Command = serde_json::from_str(&request).unwrap();
        tx.send(command).await;
    }    

}


/*
pub fn get_func(config: &Config, state: &State) -> Result<Func, LaboriError> {
    match get_params(config, state, ":FUNC?") {
        Ok(s) =>  Ok(Func::from(s.as_ref())),
        Err(e) => Err(e),
    }
}

pub fn set_func(config: &Config, state: &State, func: Func) -> Result<(), LaboriError> {
    println!("set func");
    if let Err(e) = set_params(config, state, ":FUNC", func.into()) {
        Err(e)
    } else {
        Ok(())
    }
}

pub fn get_interval(config: &Config, state: &State) -> Result<f32, LaboriError> {
    match get_params(config, state, ":GATE:TIME?") {
        Ok(s) =>  match s.parse::<f32>() {
            Ok(f) => Ok(f),
            Err(e) => Err(LaboriError::ParseFloatError(e))
        }
        Err(e) => Err(e),
    }
}

pub fn set_interval(config: &Config, state: &State, interval: f32) -> Result<(), LaboriError> {
    println!("set interval");
    if let Err(e) = set_params(config, state, ":GATE:TIME", &interval.to_string()) {
        Err(e)
    } else {
        println!("set interval OK");
        Ok(())
    }
}









fn _u8_to_func(byte: u8) -> Func {
    match byte {
        0u8 => Func::FINA,
        1u8 => Func::FINB,
        2u8 => Func::FINC,
        _ => Func::FINA,
    }
}

fn _u8_to_interval(byte: u8) -> f32 {
    match byte {
        0u8 => 0.00001,
        1u8 => 0.0001,
        2u8 => 0.001,
        3u8 => 0.01,
        4u8 => 0.1,
        5u8 => 1.0,
        6u8 => 10.0,
        _ => 0.1,    
    }
}

*/
