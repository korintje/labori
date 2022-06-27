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

fn send_to_machine(stream: &TcpStream, query: Vec<u8>) -> Result<(), LaboriError> {
    let mut writer = BufWriter::new(stream);
    match writer.write(&query) {
        Ok(_) => println!("Sent query: {:?}", query),
        Err(e) => return Err(LaboriError::TCPSendError(e.to_string()))
    }
    writer.flush().unwrap();
    Ok(())
}

fn receive_from_machine(stream: &TcpStream) -> Result<String, LaboriError> {
    let mut reader = BufReader::new(stream);
    let mut buff = vec![0; 1024];
    let n = match reader.read(&mut buff) {
        Ok(n) => n,
        Err(e) => return Err(LaboriError::TCPReceiveError(e.to_string()))
    };
    let response_ba = &buff[0..n];
    println!("Received response: {:?}", response_ba);
    if response_ba.last() != Some(&10u8) {
        return Err(LaboriError::TCPReceiveError("Broken message received".to_string()))
    }
    let response_ba = &response_ba[..response_ba.len()-1];
    let response = ASCII.decode(response_ba, DecoderTrap::Replace).unwrap();
    Ok(response)
}

fn get_params(config: &Config, state: &State, query: &str) -> Result<String, LaboriError> {

    // Reject request if the system in measuring
    if let State::Running = state {
        println!("Now in measuring");
        return Err(LaboriError::InMeasuringError("Now in measuring".to_string()))
    }

    // Get params
    let query_ba = ASCII.encode(&(query.to_string() + "\n"), EncoderTrap::Replace).unwrap();
    match std::net::TcpStream::connect(&config.device_addr) {
        Err(e) => return Err(LaboriError::TCPConnectionError(e)),
        Ok(stream) => {
            let _ = send_to_machine(&stream, query_ba)?;
            let response = receive_from_machine(&stream)?;
            Ok(response)
        }
    }

}

fn set_params(config: &Config, state: &State, query: &str, param: &str) -> Result<(), LaboriError> {

    // Reject request if the system in measuring
    println!("query: {}", query);
    println!("param: {}", param);
    if let State::Running = state {
        println!("Now in measuring");
        return Err(LaboriError::InMeasuringError("Now in measuring".to_string()))
    }

    // Set params
    let query = query.to_string() + " " + &param.to_string() + "\n";
    let query_ba = ASCII.encode(&query, EncoderTrap::Replace).unwrap();
    match std::net::TcpStream::connect(&config.device_addr) {
        Err(e) => return Err(LaboriError::TCPConnectionError(e)),
        Ok(stream) => {
            let _ = send_to_machine(&stream, query_ba)?;
            std::thread::sleep(std::time::Duration::from_secs(1));
            Ok(())
        }
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
