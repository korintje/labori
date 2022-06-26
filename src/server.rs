use std::net::{TcpListener, TcpStream};
use std::io::{BufReader, Write, Read, BufWriter};
use tokio::sync::mpsc;
use crate::{model, error::LaboriError};
use encoding::{Encoding, EncoderTrap, DecoderTrap};
use encoding::all::ASCII;
use crate::db;
use crate::load;
use std::path;

struct Config {  
    measurement_method: String,
    sample_rate: f32,
    sampling_time_millisec: i32,
    connected: bool,
}

pub struct APIServer {
    in_measuring: bool,
    device_name: String,
    device_addr: String,
    api_port: u16,
    // db_connection: sqlx::SqliteConnection,
}


impl APIServer {

    pub fn get_func(&self) -> Result<model::Func, LaboriError> {
        match self.get_params("FUNC?") {
            Ok(s) =>  Ok(model::Func::from(s.as_ref())),
            Err(e) => Err(e),
        }
    }

    pub fn set_func(&self, func: model::Func) -> Result<(), LaboriError> {
        if let Err(e) = self.set_params("FUNC", func.into()) {
            Err(e)
        } else {
            Ok(())
        }
    }

    pub fn get_interval(&self) -> Result<f32, LaboriError> {
        match self.get_params("GATE:TIME?") {
            Ok(s) =>  match s.parse::<f32>() {
                Ok(f) => Ok(f),
                Err(e) => Err(LaboriError::ParseFloatError(e))
            }
            Err(e) => Err(e),
        }
    }

    pub fn set_interval(&self, interval: f32) -> Result<(), LaboriError> {
        if let Err(e) = self.set_params("GATE:TIME", &interval.to_string()) {
            Err(e)
        } else {
            Ok(())
        }
    }

    fn send(&self, stream: &TcpStream, query: Vec<u8>) -> Result<(), LaboriError> {
        let mut writer = BufWriter::new(stream);
        match writer.write(&query) {
            Ok(_) => println!("Sent query: {:?}", query),
            Err(e) => return Err(LaboriError::TCPSendError(e.to_string()))
        }
        writer.flush().unwrap();
        Ok(())
    }

    fn receive(&self, stream: &TcpStream) -> Result<String, LaboriError> {
        let mut reader = BufReader::new(stream);
        let mut buff = vec![0; 1024];
        let n = match reader.read(&mut buff) {
            Ok(n) => n,
            Err(e) => return Err(LaboriError::TCPReceiveError(e.to_string()))
        };
        let response_ba = &buff[0..n];
        if response_ba.last() != Some(&10u8) {
            return Err(LaboriError::TCPReceiveError("Broken message received".to_string()))
        }
        let response_ba = &response_ba[..response_ba.len()-1];
        let response = ASCII.decode(response_ba, DecoderTrap::Replace).unwrap();
        Ok(response)
    }

    fn get_params(&self, query: &str) -> Result<String, LaboriError> {

        // Reject request if the system in measuring
        if self.in_measuring {
            return Err(LaboriError::InMeasuringError("Now in measuring".to_string()))
        }

        // Get params
        let query_ba = ASCII.encode(query, EncoderTrap::Replace).unwrap();
        match std::net::TcpStream::connect(&self.device_addr) {
            Err(e) => return Err(LaboriError::TCPConnectionError(e)),
            Ok(stream) => {
                let _ = self.send(&stream, query_ba)?;
                let response = self.receive(&stream)?;
                Ok(response)
            }
        }

    }

    fn set_params(&self, query: &str, param: &str) -> Result<(), LaboriError> {

        // Reject request if the system in measuring
        if self.in_measuring {
            return Err(LaboriError::InMeasuringError("Now in measuring".to_string()))
        }

        // Get params
        let query = query.to_string() + " " + &param.to_string();
        let query_ba = ASCII.encode(&query, EncoderTrap::Replace).unwrap();
        match std::net::TcpStream::connect(&self.device_addr) {
            Err(e) => return Err(LaboriError::TCPConnectionError(e)),
            Ok(stream) => {
                let _ = self.send(&stream, query_ba)?;
                Ok(())
            }
        }

    }

    fn _u8_to_func(&self, byte: u8) -> model::Func {
        match byte {
            0u8 => model::Func::FINA,
            1u8 => model::Func::FINB,
            2u8 => model::Func::FINC,
            _ => model::Func::FINA,
        }
    }

    fn _u8_to_interval(&self, byte: u8) -> f32 {
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

    async fn launch(&self, db_connection: sqlx::SqliteConnection) 
    -> Result<(), LaboriError> {

        let (tx0, rx0) = mpsc::channel(1024);
        let poll_handle = tokio::spawn(load::get_data_tcp(tx0));
        let db_handle = tokio::spawn(db::save_db(rx0, db_connection));
        let results = tokio::join!(poll_handle, db_handle);
        match results {
            (Ok(_), Ok(_)) => Ok(()),
            (Err(e), _) => return Err(LaboriError::from(e)),
            (_, Err(e)) => return Err(LaboriError::from(e)),
        }

    }

    async fn listen(mut self) -> Result<(), LaboriError> {

        let listener = TcpListener::bind(format!("127.0.0.1:{}", self.api_port))?;
        loop {
            let (stream, _addr) = listener.accept()?;
            let mut reader = BufReader::new(&stream);
            let mut buff = vec![0; 1024];
            let n = reader.read(&mut buff).expect("API RECEIVE FAILURE!!!");

            // Frequently used command 
            // Initial 2 bytes are reserved for future
            let cmd = &buff[2]; // 0: Stop, 1: Start, 2: GET_FUNC, 3: GET_INTERVAL
            let func_ba = &buff[3]; // 0: FINA, 1: FINB, 2: FINC
            let interval_ba = &buff[4]; //  0: 10us, 1: 100us, 2: 1ms, 3: 10 ms, 4: 100ms, 5: 1s, 6: 10s

            match cmd {
                &0u8 => self.in_measuring = false,
                &1u8 => {
                    
                    let dbpath = "test.db";
                    if ! path::Path::new(&dbpath).exists() {
                        db::create_db(&dbpath).await?;
                    }
                    let conn = db::connect_db(&dbpath).await?;
                    let conn = db::prepare_tables(conn).await?;

                    self.in_measuring = true;
                    let func = self._u8_to_func(*func_ba);
                    self.set_func(func);
                    let interval = self._u8_to_interval(*interval_ba);
                    self.set_interval(interval);
                    self.launch(conn);
                }
                &2u8 => { self.get_func(); },
                &3u8 => { self.get_interval(); },
                _ => (),
            }
        }
    }
}


/*
pub async fn api_server(port: u16, tx: mpsc::Sender<model::Command>) -> std::io::Result<()> {


    let listener = TcpListener::bind(format!("127.0.0.1:{}", port))?;
    // accept connections and process them serially
    loop {
        let (stream, _addr) = listener.accept()?;
        let mut reader = BufReader::new(&stream);
        let mut buff = vec![0; 1024];
        let n = reader.read(&mut buff).expect("API RECEIVE FAILURE!!!");



        // let mut reader = BufReader::new(&stream);
        // fprocess_socket(socket);
    }
    
    Ok(())

}
*/