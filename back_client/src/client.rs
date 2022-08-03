use crate::model::{Command, Response, Success, Failure};
use crate::config::Config;
use crate::error::LaboriError;
use crate::logger;
use std::net::TcpStream;
use tokio::sync::mpsc;
use std::io::{BufReader, Write, Read, BufWriter};
use encoding::{Encoding, EncoderTrap, DecoderTrap};
use encoding::all::ASCII;
use chrono::Local;

pub async fn connect(
    config: Config,
    tx_to_server: mpsc::Sender<Response>,
    mut rx_from_server: mpsc::Receiver<Command>,
) -> Result<(), LaboriError> {

    let device_name = &config.device_name;
    let device_addr = &config.device_addr;

    while let Some(cmd_obj) = rx_from_server.recv().await {
            
        // Check whether inputed command is correct
        let cmd = match cmd_obj.into_cmd() {
            Err(e) => {
                tx_to_server.send(
                    Response::Failure(Failure::InvalidCommand(e.to_string()))
                ).await.unwrap();
                continue
            },
            Ok(cmd) => cmd,
        };

        match cmd_obj {
            Command::Get { key: _ } => {
                match get_stream(device_addr) {
                    Some(stream) => {
                        send_cmd(&stream, &tx_to_server, &cmd).await;
                        tx_to_server.send(
                            get_response(&stream).await
                        ).await.unwrap();
                    },
                    None => respond_empty_stream(&tx_to_server).await,
                }
            },
            Command::Set { key: _, value: val } => {
                match get_stream(device_addr) {
                    Some(stream) => {
                        send_cmd(&stream, &tx_to_server, &cmd).await;
                        tx_to_server.send(
                            Response::Success(Success::SetValue(val))
                        ).await.unwrap();
                    },
                    None => respond_empty_stream(&tx_to_server).await,
                }
            },
            Command::Run{} => {
                match get_stream(device_addr) {
                    Some(stream) => {
                        send_cmd(&stream, &tx_to_server, &cmd).await;
                        poll(&device_name, &stream, &tx_to_server, &mut rx_from_server).await;
                    },
                    None => respond_empty_stream(&tx_to_server).await,
                }
            },
            Command::Stop{} => {
                tx_to_server.send(
                    Response::Failure(Failure::NotRunning("Measurement not running".to_string()))
                ).await.unwrap();
            }
        }
    }
    Ok(())
}

fn get_stream(device_addr: &str) -> Option<TcpStream> {
    match std::net::TcpStream::connect(device_addr) {
        Err(e) => {
            println!("Failed to connect TCP server: {}", e);
            None
        },
        Ok(stream) => {
            println!("Successfully connected TCP server");
            Some(stream)
        },
    }
}

async fn respond_empty_stream(tx: &mpsc::Sender<Response>) {
    tx.send(
        Response::Failure(Failure::EmptyStream(
            "Failed to get TCP stream with counter".to_string()
        ))
    ).await.unwrap();
}

async fn send_cmd(stream: &TcpStream, tx: &mpsc::Sender<Response>, cmd: &str) {
    let cmd_ba = ASCII.encode(cmd, EncoderTrap::Replace).unwrap();
    let mut writer = BufWriter::new(stream);
    match writer.write(&cmd_ba) {
        Ok(_) => println!("Sent query: {:?}", &cmd_ba),
        Err(e) => {
            tx.send(
                Response::Failure(Failure::CommandNotSent(e.to_string()))
            ).await.unwrap();
        },
    }
    if let Err(e) = writer.flush() {
        println!("Failed to flush TCP writer: {}", e);
    }
}

async fn get_response(stream: &TcpStream) -> Response {
    let mut reader = BufReader::new(stream);
    let mut buff = vec![0; 1024];
    match reader.read(&mut buff) {
        Ok(n) => {
            let response_ba = &buff[0..n];
            println!("Received response: {:?}", response_ba);
            if response_ba.last() != Some(&10u8) {
                Response::Failure(Failure::InvalidReturn("No LF in the end of the response".to_string()))
            } else {
                let response_ba = &response_ba[..response_ba.len()-1];
                let response = ASCII.decode(response_ba, DecoderTrap::Replace).unwrap();
                Response::Success(Success::GotValue(response))
            }
        },
        Err(e) => {
            Response::Failure(Failure::MachineNotRespond(e.to_string()))
        },
    }    
}

async fn poll(
    device_name: &str,
    stream: &TcpStream,
    tx_to_server: &mpsc::Sender<Response>,
    rx_from_server: &mut mpsc::Receiver<Command>,
  ) {

    // Get interval value
    let cmd = Command::Get { key: "Interval".to_string() }.into_cmd().unwrap();
    send_cmd(&stream, &tx_to_server, &cmd).await;
    let response = get_response(&stream).await;
    let interval_str = match response {
        Response::Success(Success::GotValue(val)) => val,
        _ => panic!("Could not to get interval value from machine"),
    };
    let interval = interval_str.parse().unwrap();

    // Determine polling duration
    let duration;
    if interval <= 0.001 {
        duration = 10;
    } else if interval <= 0.01 {
        duration = 100;        
    } else if interval <= 0.1 {
        duration = 110;
    } else if interval <= 1.0 {
        duration = 1100;
    } else {
        duration = 11000;
    }
    let mut polling_interval = tokio::time::interval(
        tokio::time::Duration::from_millis(duration)
    );

    // Spawn logger
    let table_name = &Local::now().format("%Y-%m-%dT%H:%M:%S");
    let (tx_to_logger, rx_from_client) = mpsc::channel(1024);
    let log_handle = tokio::spawn(
        logger::log(device_name.to_string(), table_name.to_string(), interval, rx_from_client)
    );
  
    // Prepare command bytes
    let polling_cmd = ":LOG:DATA?\n";
    let polling_cmd = ASCII.encode(polling_cmd, EncoderTrap::Replace).unwrap();
  
    // Prepare buffers
    let mut reader = BufReader::new(stream);
    let mut writer = BufWriter::new(stream);

    // Respond that the measurement has started
    tx_to_server.send(
        Response::Success(Success::SaveTable(table_name.to_string()))
    ).await.unwrap();
    
    // Data polling loop
    loop {

        writer.write(&polling_cmd).unwrap();
        writer.flush().unwrap();
  
        let mut buff = vec![0; 1024];
        let n = reader.read(&mut buff).unwrap();
        // println!("{}\r", n);
        
        if n >= 2 {
            if let Err(e) = tx_to_logger.send(buff[..n].to_vec()).await {
                println!("Failed to send {}", e)
            };
        }
        
        // Controll polling interval
        match rx_from_server.try_recv() {
            Ok(cmd) => {
                match cmd {
                    Command::Stop {} => {
                        if let Err(e) = tx_to_logger.send(vec![4u8]).await {
                            println!("Failed to kill logger {}", e)
                        };
                        break
                    },
                    _ => tx_to_server.send(
                        Response::Failure(
                            Failure::Busy{
                                table_name: table_name.to_string(),
                                interval: interval_str.clone(),
                            }
                        )
                    ).await.unwrap()
                } 
            },
            Err(_) => (),
        }

        polling_interval.tick().await;

    }

    if let Err(e) = log_handle.await.unwrap() {
        tx_to_server.send(
            Response::Failure(Failure::SaveDataFailed(
                e.to_string()
            ))
        ).await.unwrap();
    }else{
        tx_to_server.send(
            Response::Success(Success::Finished(
                "Measurement successfully finished".to_string()
            ))
        ).await.unwrap();
    }
}
