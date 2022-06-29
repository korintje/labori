use crate::model::{Command, PollerCommand, Response, PollerResponse, Success, Failure};
use crate::config::Config;
use crate::error::LaboriError;
use crate::logger;
use std::net::TcpStream;
use std::task::Poll;
use tokio::sync::mpsc;
use std::io::{BufReader, Write, Read, BufWriter};
use encoding::{Encoding, EncoderTrap, DecoderTrap};
use encoding::all::ASCII;
use tokio::time::{sleep, Duration};
use chrono::Local;

pub async fn connect(
    config: Config,
    tx_to_server: mpsc::Sender<Response>,
    tx_to_poller: mpsc::Sender<PollerCommand>,
    mut rx_from_server: mpsc::Receiver<Command>,
    mut rx_from_poller: mpsc::Receiver<PollerResponse>,
) -> Result<(), LaboriError> {

    let stream = match std::net::TcpStream::connect(&config.device_addr) {
        Err(e) => return Err(LaboriError::TCPConnectionError(e)),
        Ok(stream) => stream,
    };

    let device_name = config.device_name;
    // let table_name = "table_name".to_string();

    while let Some(cmd_obj) = rx_from_server.recv().await {

        let cmd = match cmd_obj.into_cmd() {
            Err(e) => {
                tx_to_server.send(
                    Response::Failed(Failure::InvalidCommand(e.to_string()))
                ).await.unwrap();
                continue
            },
            Ok(cmd) => cmd,
        };

        match cmd_obj {
            Command::Get { key: _ } => {
                if let Err(e) = send_cmd(&stream, &cmd) {
                    tx_to_server.send(
                        Response::Failed(Failure::CommandNotSent(e.to_string()))
                    ).await.unwrap();
                }
                match get_response(&stream) {
                    Err(e) => tx_to_server.send(
                        Response::Failed(Failure::MachineNotRespond(e.to_string()))
                    ).await.unwrap(),
                    Ok(res) => tx_to_server.send(
                        Response::Success(Success::GotValue(res))
                    ).await.unwrap(),
                };
            },
            Command::Set { key: _, value: val } => {
                if let Err(e) = send_cmd(&stream, &cmd) {
                    tx_to_server.send(
                        Response::Failed(Failure::CommandNotSent(e.to_string()))
                    ).await.unwrap();
                }
                tx_to_server.send(
                    Response::Success(Success::SetValue(val))
                ).await.unwrap();
            },
            Command::Run{} => {
                match tx_to_poller.send(PollerCommand::GetState).await {
                    Err(e) => tx_to_server.send(
                        Response::Failed(Failure::PollerCommandNotSent(e.to_string()))
                    ).await.unwrap(),
                    Ok(_) => match rx_from_poller.recv().await.unwrap() {
                        PollerResponse::Running => {
                            tx_to_server.send(
                                Response::Failed(Failure::Busy(
                                    "Measurement already running".to_string()
                                ))
                            ).await.unwrap();
                        },
                        PollerResponse::Waiting => {
                            if let Err(e) = send_cmd(&stream, &cmd) {
                                tx_to_server.send(
                                    Response::Failed(Failure::CommandNotSent(e.to_string()))
                                ).await.unwrap();
                            }
                            let table_name = Local::now().format("%Y%m%d%H%M%S");
                            tx_to_poller.send(
                                PollerCommand::Run{table_name: table_name.to_string()}
                            ).await.unwrap();
                            tx_to_server.send(
                                Response::Success(Success::SaveTable(table_name.to_string()))
                            ).await.unwrap();
                        },
                    } 
                }
            },
            Command::Stop{} => {
                match tx_to_poller.send(PollerCommand::GetState).await {
                    Err(e) => tx_to_server.send(
                        Response::Failed(Failure::PollerCommandNotSent(e.to_string()))
                    ).await.unwrap(),
                    Ok(_) => match rx_from_poller.recv().await.unwrap() {
                        PollerResponse::Running => {
                            tx_to_poller.send(PollerCommand::Stop).await.unwrap();
                            tx_to_server.send(
                                Response::Success(Success::Finished("Measurement finished".to_string()))
                            ).await.unwrap();                            
                            if let Err(e) = send_cmd(&stream, &cmd) {
                                tx_to_server.send(
                                    Response::Failed(Failure::CommandNotSent(e.to_string()))
                                ).await.unwrap();
                            }                            
                        },
                        PollerResponse::Waiting => {
                            tx_to_server.send(
                                Response::Failed(Failure::NotRunning("Measurement not running".to_string()))
                            ).await.unwrap();
                        },
                    },            
                }
            }
        }
    }
    Ok(())
}

                /*
                match poll(&stream, &tx_to_server, &tx_to_logger, &mut rx).await {
                    Ok(_) => tx_to_server.send(
                        Response::Success(Success::Finished)
                    ).await.unwrap(),
                    Err(e) => tx_to_server.send(
                        Response::Failed(Failure::ErrorInRunning(e.to_string()))
                    ).await.unwrap(),
                };
                */

fn send_cmd(stream: &TcpStream, cmd: &str) -> Result<(), LaboriError> {
    let cmd_ba = ASCII.encode(cmd, EncoderTrap::Replace).unwrap();
    let mut writer = BufWriter::new(stream);
    match writer.write(&cmd_ba) {
        Ok(_) => println!("Sent query: {:?}", &cmd_ba),
        Err(e) => return Err(LaboriError::TCPSendError(e.to_string()))
    }
    writer.flush().unwrap();
    Ok(())
}

fn get_response(stream: &TcpStream) -> Result<String, LaboriError> {
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


