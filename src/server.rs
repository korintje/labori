use std::net::{TcpListener, TcpStream};
use std::io::{BufReader, Write, Read, BufWriter};
use tokio::sync::mpsc;
use crate::error::LaboriError;
use crate::config::Config;
use crate::model::{Command, Response};
use serde_json;

pub async fn serve(
    config: Config,
    tx: mpsc::Sender<Command>,
    mut rx: mpsc::Receiver<Response>,
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
        let n = match reader.read(&mut buff) {
            Ok(n) => n,
            Err(e) => {
                write_response(&mut writer, Response::InvalidRequest(
                    format!("Failed to read request: {:?}", e)
                ));
                continue
            },
        };
        // let request = ASCII.decode(&buff[0..n], DecoderTrap::Replace).unwrap();
        let request = match std::str::from_utf8(&buff[0..n]) {
            Ok(r) => r,
            Err(e) => {
                write_response(&mut writer, Response::InvalidRequest(
                    format!("Failed to decode requesrt from bytes: {:?} : {:?}", &buff[0..n], e)
                ));
                continue
            },
         };
        let command: Command = match serde_json::from_str(&request){
            Ok(s) => s,
            Err(e) => {
                write_response(&mut writer, Response::InvalidRequest(
                    format!("Failed to convert stirng to command: {:?} : {:?}", &request, e)
                ));
                continue
            },
        };
        match tx.send(command).await {
            Ok(_) => (),
            Err(e) => {
                write_response(&mut writer, Response::SignalFailed(e.to_string()));
                continue
            },
        };
        match rx.recv().await {
            Some(response) => {
                let response_str = match serde_json::to_string(&response) {
                    Ok(r) => r,
                    Err(e) => {
                        write_response(&mut writer, Response::InvalidReturn(e.to_string()));
                        continue
                    },
                };
                let response_ba = response_str.as_bytes();
                match writer.write(response_ba) {
                    Ok(_) => (),
                    Err(e) => return Err(LaboriError::APISendError(e.to_string()))
                }
            },
            None => (),
        }
    }    

}

fn write_response(writer: &mut BufWriter<&TcpStream>, response: Response) {
    let response_str = serde_json::to_string(&response).unwrap();
    let response_ba = response_str.as_bytes();
    match writer.write(response_ba) {
        Ok(_) => (),
        Err(e) => println!(
            "Failed to return response: {}. Error: {}", response_str, e
        ),
    }    
}