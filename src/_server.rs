/*
use std::net::{TcpListener, TcpStream};
use std::io::{BufReader, Write, Read, BufWriter};
use tokio::sync::mpsc;
use encoding::{Encoding, EncoderTrap, DecoderTrap};
use encoding::all::ASCII;
use crate::error::LaboriError;
use crate::config::Config;
use crate::model::{Signal, Func, State};

pub async fn serve(config: Config, tx: mpsc::Sender<Signal>) -> Result<(), LaboriError> {
    
    let mut state = State::Holded;
    let listener = TcpListener::bind(format!("127.0.0.1:{}", config.api_port))?;
    loop {
        println!("Waiting for clients");
        let (stream, _addr) = listener.accept()?;
        println!("A Client has connected");
        let mut reader = BufReader::new(&stream);
        let mut writer = BufWriter::new(&stream);
        let mut buff = vec![0; 1024];
        let _n = reader.read(&mut buff).expect("API RECEIVE FAILURE!!!");

        // Frequently used command 
        // Initial 2 bytes are reserved for future
        let cmd = &buff[2]; // 0: Stop, 1: Start, 2: GET_FUNC, 3: GET_INTERVAL
        let func_ba = &buff[3]; // 0: FINA, 1: FINB, 2: FINC
        let interval_ba = &buff[4]; //  0: 10us, 1: 100us, 2: 1ms, 3: 10 ms, 4: 100ms, 5: 1s, 6: 10s

        match cmd {
            &0u8 => {
                match tx.send(Signal::Stop).await {
                    Ok(_) => state = State::Holded,
                    Err(e) => return Err(LaboriError::from(e)),
                };
            }
            &1u8 => {
                let func = _u8_to_func(*func_ba);
                if let Err(e) = set_func(&config, &state, func) {
                    writer.write(format!("Error: {}", e).as_bytes()).unwrap();
                    writer.flush().unwrap();
                };
                let interval = _u8_to_interval(*interval_ba);
                if let Err(e) = set_interval(&config, &state, interval) {
                    writer.write(format!("Error: {}", e).as_bytes()).unwrap();
                    writer.flush().unwrap();
                };
                match tx.send(Signal::Start).await{
                    Ok(_) => {
                        state = State::Running;
                        println!("Start signal sent")
                    }
                    Err(e) => return Err(LaboriError::from(e)),
                }
            }
            &2u8 => {
                match get_func(&config, &state) {
                    Ok(f) => {
                        let f_str: &str = f.into();
                        writer.write(f_str.as_bytes()).unwrap();
                        writer.flush().unwrap();
                    },
                    Err(e) => return Err(LaboriError::from(e)),
                }
            },
            &3u8 => {
                match get_interval(&config, &state) {
                    Ok(i) => {
                        writer.write(&i.to_le_bytes()).unwrap();
                        writer.flush().unwrap();
                    },
                    Err(e) => return Err(LaboriError::from(e)),
                }
            },
            _ => (),
        }
    }    

}


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