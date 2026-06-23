use std::net::{TcpListener, TcpStream};
use std::io::{BufRead, BufReader, Write, BufWriter};
use std::time::Duration;
use tokio::sync::mpsc;
use crate::error::LaboriError;
use crate::config::Config;
use crate::model::{Command, Response, Failure};
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
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        let mut reader = BufReader::new(&stream);
        let mut writer = BufWriter::new(&stream);
        let mut buff = Vec::new();
        let n = match reader.read_until(b'\n', &mut buff) {
            Ok(n) if n <= 64 * 1024 => n,
            Ok(_) => {
                write_response(&mut writer, Response::Failure(
                    Failure::InvalidRequest(
                        "Request exceeds the 64 KiB limit".to_string()
                    )
                ));
                continue
            },
            Err(e) => {
                write_response(&mut writer, Response::Failure(
                    Failure::InvalidRequest(
                        format!("Failed to read request: {:?}", e)
                    )
                ));
                continue
            },
        };
        // let request = ASCII.decode(&buff[0..n], DecoderTrap::Replace).unwrap();
        let request = match std::str::from_utf8(&buff[0..n]) {
            Ok(r) => r.trim_end_matches(|c| c == '\r' || c == '\n'),
            Err(e) => {
                write_response(&mut writer, Response::Failure(
                    Failure::InvalidRequest(
                        format!("Failed to decode requesrt from bytes: {:?} : {:?}", &buff[0..n], e)
                    )
                ));
                continue
            },
        };
        let command: Command = match serde_json::from_str(&request){
            Ok(s) => s,
            Err(e) => {
                write_response(&mut writer, Response::Failure(
                    Failure::InvalidRequest(
                        format!("Failed to convert stirng to command: {:?} : {:?}", &request, e)
                    )
                ));
                continue
            },
        };
        match tx.send(command).await {
            Ok(_) => (),
            Err(e) => {
                write_response(&mut writer, Response::Failure(
                    Failure::SignalFailed(e.to_string())
                ));
                continue
            },
        };
        match rx.recv().await {
            Some(response) => {
                let response_str = match serde_json::to_string(&response) {
                    Ok(r) => r,
                    Err(e) => {
                        write_response(&mut writer, Response::Failure(
                            Failure::InvalidReturn(e.to_string())
                        ));
                        continue
                    },
                };
                let response_ba = response_str.as_bytes();
                match writer.write_all(response_ba) {
                    Ok(_) => {
                        if let Err(e) = writer.write_all(b"\n").and_then(|_| writer.flush()) {
                            println!("Failed to send API response: {}", e);
                        }
                    },
                    Err(e) => println!("Failed to send API response: {}", e),
                }
            },
            None => (),
        }
    }    

}

fn write_response(writer: &mut BufWriter<&TcpStream>, response: Response) {
    let response_str = serde_json::to_string(&response).unwrap();
    let response_ba = response_str.as_bytes();
    match writer.write_all(response_ba) {
        Ok(_) => {
            if let Err(e) = writer.write_all(b"\n").and_then(|_| writer.flush()) {
                println!(
                    "Failed to return response: {}. Error: {}", response_str, e
                );
            }
        },
        Err(e) => println!(
            "Failed to return response: {}. Error: {}", response_str, e
        ),
    }    
}
