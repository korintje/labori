use crate::model::{Command, Response, Success, Failure};
use crate::config::Config;
use crate::error::LaboriError;
use crate::logger;
use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc;
use std::io::{BufRead, BufReader, Write, BufWriter};
use encoding::{Encoding, EncoderTrap, DecoderTrap};
use encoding::all::ASCII;
use chrono::Local;
use std::time::{Duration, Instant};
use rppal::gpio::Gpio;

const GPIO_PINS: [u8;6] = [17, 27, 22, 23, 24, 25];
static TABLE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub async fn connect(
    config: Config,
    tx_to_server: mpsc::Sender<Response>,
    mut rx_from_server: mpsc::Receiver<Command>,
) -> Result<(), LaboriError> {

    let device_addr = &config.device_addr;
    let database_path = &config.database_path;

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
                        if let Err(e) = send_cmd(&stream, &cmd) {
                            respond_command_not_sent(&tx_to_server, e).await;
                            continue;
                        }
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
                        if let Err(e) = send_cmd(&stream, &cmd) {
                            respond_command_not_sent(&tx_to_server, e).await;
                            continue;
                        }
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
                        if let Err(e) = send_cmd(&stream, &cmd) {
                            respond_command_not_sent(&tx_to_server, e).await;
                            continue;
                        }
                        poll(
                            database_path,
                            &stream,
                            &tx_to_server,
                            &mut rx_from_server
                        ).await;
                    },
                    None => respond_empty_stream(&tx_to_server).await,
                }
            },
            Command::RunExt{ duration: duration_str } => {
                match get_stream(device_addr) {
                    Some(stream) => {
                        if let Err(e) = send_cmd(&stream, &cmd) {
                            respond_command_not_sent(&tx_to_server, e).await;
                            continue;
                        }
                        let duration_sec: f64 = duration_str.parse::<f64>().map_err(|_| {
                            LaboriError::CommandParseError(
                                format!("Invalid duration: {}", duration_str)
                            )
                        })?;
                        poll_ext(
                            database_path,
                            &stream,
                            &tx_to_server,
                            &mut rx_from_server,
                            duration_sec
                        ).await;
                    },
                    None => respond_empty_stream(&tx_to_server).await,
                }
            },
            Command::RunMulti{ channels, interval } => {
                match get_stream(device_addr) {
                    Some(stream) => {
                        if let Err(e) = send_cmd(&stream, &cmd) {
                            respond_command_not_sent(&tx_to_server, e).await;
                            continue;
                        }
                        poll_multi(
                            database_path,
                            &stream,
                            &tx_to_server,
                            &mut rx_from_server,
                            channels,
                            interval,
                            config.gpio_settle_millis
                        ).await;
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
            if let Err(e) = stream.set_read_timeout(Some(Duration::from_millis(250))) {
                println!("Failed to set TCP read timeout: {}", e);
                return None
            }
            if let Err(e) = stream.set_write_timeout(Some(Duration::from_secs(5))) {
                println!("Failed to set TCP write timeout: {}", e);
                return None
            }
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

async fn respond_command_not_sent(tx: &mpsc::Sender<Response>, error: String) {
    let _ = tx.send(
        Response::Failure(Failure::CommandNotSent(error))
    ).await;
}

fn send_cmd(stream: &TcpStream, cmd: &str) -> Result<(), String> {
    let cmd_ba = ASCII.encode(cmd, EncoderTrap::Replace).unwrap();
    let mut writer = BufWriter::new(stream);
    writer.write_all(&cmd_ba).map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())?;
    println!("Sent query: {:?}", &cmd_ba);
    Ok(())
}

async fn get_response(stream: &TcpStream) -> Response {
    let mut reader = BufReader::new(stream);
    let mut response_ba = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match reader.read_until(b'\n', &mut response_ba) {
            Ok(0) => return Response::Failure(Failure::EmptyStream(
                "The counter closed the connection without a response".to_string()
            )),
            Ok(_) => {
                println!("Received response: {:?}", response_ba);
                if response_ba.last() != Some(&10u8) {
                    return Response::Failure(Failure::InvalidReturn(
                        "No LF in the end of the response".to_string()
                    ))
                }
                let response_ba = &response_ba[..response_ba.len() - 1];
                let response = ASCII.decode(response_ba, DecoderTrap::Replace).unwrap();
                return Response::Success(Success::GotValue(response))
            },
            Err(e) if matches!(
                e.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ) => {
                if Instant::now() >= deadline {
                    return Response::Failure(Failure::MachineNotRespond(
                        "Timed out waiting for the counter response".to_string()
                    ))
                }
            },
            Err(e) => {
                return Response::Failure(Failure::MachineNotRespond(e.to_string()))
            }
        }
    }
}

enum ControlledRead {
    Data(Vec<u8>),
    Stop,
    Error(String),
}

async fn respond_busy(
    tx: &mpsc::Sender<Response>,
    table_name: &str,
    interval: &str,
) {
    let _ = tx.send(
        Response::Failure(Failure::Busy {
            table_name: table_name.to_string(),
            interval: interval.to_string(),
        })
    ).await;
}

async fn read_with_control(
    reader: &mut BufReader<&TcpStream>,
    rx: &mut mpsc::Receiver<Command>,
    tx: &mpsc::Sender<Response>,
    table_name: &str,
    interval: &str,
    timeout: Duration,
) -> ControlledRead {
    let deadline = Instant::now() + timeout;
    let mut data = Vec::new();
    loop {
        match reader.read_until(b'\n', &mut data) {
            Ok(0) => return ControlledRead::Error(
                "The counter closed the connection".to_string()
            ),
            Ok(_) => return ControlledRead::Data(data),
            Err(e) if matches!(
                e.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ) => {
                match rx.try_recv() {
                    Ok(Command::Stop {}) => return ControlledRead::Stop,
                    Ok(_) => respond_busy(tx, table_name, interval).await,
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        return ControlledRead::Stop
                    },
                    Err(mpsc::error::TryRecvError::Empty) => (),
                }
                if Instant::now() >= deadline {
                    return ControlledRead::Error(
                        "Timed out waiting for the counter response".to_string()
                    )
                }
            },
            Err(e) => return ControlledRead::Error(e.to_string()),
        }
    }
}

async fn wait_with_control(
    duration: Duration,
    rx: &mut mpsc::Receiver<Command>,
    tx: &mpsc::Sender<Response>,
    table_name: &str,
    interval: &str,
) -> bool {
    let deadline = tokio::time::Instant::now() + duration;
    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => return false,
            command = rx.recv() => {
                match command {
                    Some(Command::Stop {}) | None => return true,
                    Some(_) => respond_busy(tx, table_name, interval).await,
                }
            }
        }
    }
}

fn new_table_name() -> String {
    let sequence = TABLE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!(
        "{}-{:04}",
        Local::now().format("%Y-%m-%dT%H-%M-%S%.6f"),
        sequence % 10_000
    )
}

async fn finish_measurement(
    log_handle: tokio::task::JoinHandle<Result<(), LaboriError>>,
    terminal_failure: Option<Failure>,
    tx: &mpsc::Sender<Response>,
    rx: &mut mpsc::Receiver<Command>,
    stop_requested: bool,
) {
    let logger_result = match log_handle.await {
        Ok(result) => result.map_err(|e| e.to_string()),
        Err(e) => Err(e.to_string()),
    };
    if !stop_requested {
        // A spontaneous device/logger failure has no waiting API request.
        // Consume the next request and use it to report the terminal failure,
        // avoiding a stale response in the shared response queue.
        if rx.recv().await.is_none() {
            return
        }
    }
    if let Some(failure) = terminal_failure {
        let _ = tx.send(Response::Failure(failure)).await;
    } else if let Err(e) = logger_result {
        let _ = tx.send(Response::Failure(
            Failure::SaveDataFailed(e)
        )).await;
    } else {
        let _ = tx.send(Response::Success(Success::Finished(
            "Measurement successfully finished".to_string()
        ))).await;
    }
}

async fn poll(
    database_path: &str,
    stream: &TcpStream,
    tx_to_server: &mpsc::Sender<Response>,
    rx_from_server: &mut mpsc::Receiver<Command>,
  ) {

    // Get interval value
    let cmd = Command::Get { key: "Interval".to_string() }.into_cmd().unwrap();
    if let Err(e) = send_cmd(stream, &cmd) {
        let _ = tx_to_server.send(
            Response::Failure(Failure::CommandNotSent(e))
        ).await;
        return
    }
    let response = get_response(&stream).await;
    let interval_str = match response {
        Response::Success(Success::GotValue(val)) => val,
        failure => {
            let _ = tx_to_server.send(failure).await;
            return
        },
    };
    let interval: f64 = match interval_str.parse() {
        Ok(value) => value,
        Err(e) => {
            let _ = tx_to_server.send(
                Response::Failure(Failure::InvalidReturn(e.to_string()))
            ).await;
            return
        },
    };

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
    // Spawn logger
    let table_name = new_table_name();
    let (tx_to_logger, rx_from_client) = mpsc::channel(1024);
    let log_handle = tokio::spawn(
        logger::log(database_path.to_string(), table_name.clone(), interval, rx_from_client)
    );
  
    // Prepare command bytes
    let polling_cmd = ":LOG:DATA?\n";
    let polling_cmd = ASCII.encode(polling_cmd, EncoderTrap::Replace).unwrap();
    let stop_cmd = ASCII.encode(":FRUN 0\n", EncoderTrap::Replace).unwrap();
  
    // Prepare buffers
    let mut reader = BufReader::new(stream);
    let mut writer = BufWriter::new(stream);

    // Respond that the measurement has started
    tx_to_server.send(
        Response::Success(Success::SaveTable(table_name.clone()))
    ).await.unwrap();
    let mut terminal_failure = None;
    let mut stop_requested = false;
    
    // Data polling loop
    loop {

        if let Err(e) = writer.write_all(&polling_cmd).and_then(|_| writer.flush()) {
            let _ = tx_to_logger.send(vec![4u8]).await;
            terminal_failure = Some(Failure::PollerCommandNotSent(e.to_string()));
            break
        }

        match read_with_control(
            &mut reader,
            rx_from_server,
            tx_to_server,
            &table_name,
            &interval_str,
            Duration::from_secs(5)
        ).await {
            ControlledRead::Data(buff) if buff.len() >= 2 => {
                if let Err(e) = tx_to_logger.send(buff).await {
                    println!("Failed to send {}", e)
                }
            },
            ControlledRead::Data(_) => (),
            ControlledRead::Stop => {
                stop_requested = true;
                let _ = writer.write_all(&stop_cmd).and_then(|_| writer.flush());
                let _ = tx_to_logger.send(vec![4u8]).await;
                break
            },
            ControlledRead::Error(e) => {
                let _ = tx_to_logger.send(vec![4u8]).await;
                terminal_failure = Some(Failure::MachineNotRespond(e));
                break
            },
        }

        if wait_with_control(
            Duration::from_millis(duration),
            rx_from_server,
            tx_to_server,
            &table_name,
            &interval_str
        ).await {
            stop_requested = true;
            let _ = writer.write_all(&stop_cmd).and_then(|_| writer.flush());
            if let Err(e) = tx_to_logger.send(vec![4u8]).await {
                println!("Failed to send {}", e)
            }
            break
        }

    }

    finish_measurement(
        log_handle,
        terminal_failure,
        tx_to_server,
        rx_from_server,
        stop_requested
    ).await;
}


async fn poll_ext(
    database_path: &str,
    stream: &TcpStream,
    tx_to_server: &mpsc::Sender<Response>,
    rx_from_server: &mut mpsc::Receiver<Command>,
    duration: f64,
) {

    // Determine polling duration
    let interval_str = if (duration - 0.001).abs() < f64::EPSILON {
        "1.0E-3".to_string()
    } else if (duration - 0.01).abs() < f64::EPSILON {
        "10E-3".to_string()
    } else if (duration - 0.1).abs() < f64::EPSILON {
        "0.10E+0".to_string()
    } else if (duration - 1.0).abs() < f64::EPSILON {
        "1.0E+0".to_string()
    } else if (duration - 10.0).abs() < f64::EPSILON {
        "10.0E+0".to_string()
    } else {
        duration.to_string()
    };

    // Set start time and polling interval
    let start_time = Instant::now();
    // Spawn logger
    let table_name = new_table_name();
    let (tx_to_logger, rx_from_client) = mpsc::channel(1);
    let log_handle = tokio::spawn(
        logger::log_ext(
            database_path.to_string(),
            table_name.clone(),
            duration,
            rx_from_client
        )
    );
  
    // Prepare command bytes
    let polling_cmd = ":MEAS?\n";
    let polling_cmd = ASCII.encode(polling_cmd, EncoderTrap::Replace).unwrap();
  
    // Prepare buffers
    let mut reader = BufReader::new(stream);
    let mut writer = BufWriter::new(stream);

    // Respond that the measurement has started
    tx_to_server.send(
        Response::Success(Success::SaveTable(table_name.clone()))
    ).await.unwrap();
    let mut terminal_failure = None;
    let mut stop_requested = false;
    
    // Data polling loop
    loop {

        // Send polling command
        if let Err(e) = writer.write_all(&polling_cmd).and_then(|_| writer.flush()) {
            let _ = tx_to_logger.send(vec![4u8]).await;
            terminal_failure = Some(Failure::PollerCommandNotSent(e.to_string()));
            break
        }

        let buff = match read_with_control(
            &mut reader,
            rx_from_server,
            tx_to_server,
            &table_name,
            &interval_str,
            Duration::from_secs_f64(duration.max(0.001) + 5.0)
        ).await {
            ControlledRead::Data(buff) => buff,
            ControlledRead::Stop => {
                stop_requested = true;
                let _ = tx_to_logger.send(vec![4u8]).await;
                break
            },
            ControlledRead::Error(e) => {
                let _ = tx_to_logger.send(vec![4u8]).await;
                terminal_failure = Some(Failure::MachineNotRespond(e));
                break
            },
        };

        // end-measurement time
        let meas_time = start_time.elapsed().as_millis() as u64;

        // Send value to logger
        if buff.len() >= 2 {
            let mut data_vec = meas_time.to_ne_bytes().to_vec();
            data_vec.extend_from_slice(&buff);
            // println!("data size: {:?} bytes", data_vec.len());
            if let Err(e) = tx_to_logger.send(data_vec).await {
                println!("Failed to send {}", e)
            }else{
                // println!("SENT");
            };
        }

    }

    finish_measurement(
        log_handle,
        terminal_failure,
        tx_to_server,
        rx_from_server,
        stop_requested
    ).await;
}


async fn poll_multi(
    database_path: &str,
    stream: &TcpStream,
    tx_to_server: &mpsc::Sender<Response>,
    rx_from_server: &mut mpsc::Receiver<Command>,
    channels: Vec<u8>,
    interval: f64,
    gpio_settle_millis: u64,
) {

    // Setup GPIO
    let gpio = match Gpio::new() {
        Ok(gpio) => gpio,
        Err(e) => {
            let _ = tx_to_server.send(
                Response::Failure(Failure::ErrorInRunning(e.to_string()))
            ).await;
            return
        },
    };
    let mut used_pins = Vec::with_capacity(GPIO_PINS.len());
    for pin_no in GPIO_PINS {
        match gpio.get(pin_no) {
            Ok(pin) => used_pins.push(pin.into_output()),
            Err(e) => {
                for pin in used_pins.iter_mut() {
                    pin.set_low();
                }
                let _ = tx_to_server.send(
                    Response::Failure(Failure::ErrorInRunning(e.to_string()))
                ).await;
                return
            },
        }
    }
    for pin in used_pins.iter_mut() {
        pin.set_low();
    }

    // Prepare buffers
    let mut reader = BufReader::new(stream);
    let mut writer = BufWriter::new(stream);
    
    // Current channel
    let channel_count = channels.len();
    if channel_count == 0 {
        tx_to_server.send(
            Response::Failure(Failure::InvalidRequest(
                "No channels are selected".to_string()
            ))
        ).await.unwrap();
        return ()
    }
    let mut channel_index = 0usize;

    // Prepare command bytes
    let polling_cmd = ":MEAS?\n";
    let polling_cmd = ASCII.encode(polling_cmd, EncoderTrap::Replace).unwrap();

    // Spawn logger
    let table_name = new_table_name();
    let (tx_to_logger, rx_from_client) = mpsc::channel(1);
    let log_handle = tokio::spawn(
        logger::log_multi(
            database_path.to_string(),
            table_name.clone(),
            channels.clone(),
            interval,
            rx_from_client
        )
    );

    // Respond that the measurement has started
    tx_to_server.send(
        Response::Success(Success::SaveTable(table_name.clone()))
    ).await.unwrap();
    let mut terminal_failure = None;
    let mut stop_requested = false;

    // Set start time
    let start_time = Instant::now();
    
    // Data polling loop
    loop {
        let ch = channels[channel_index];

        // Set HIGH for the target GPIO pin
        let ch_idx = ch as usize;
        used_pins[ch_idx].set_high();

        // Wait for the selected input to settle.
        if wait_with_control(
            Duration::from_millis(gpio_settle_millis),
            rx_from_server,
            tx_to_server,
            &table_name,
            &interval.to_string()
        ).await {
            stop_requested = true;
            used_pins[ch_idx].set_low();
            let _ = tx_to_logger.send(vec![4u8]).await;
            break
        }

        // start-measurement time
        let start_meas_time = start_time.elapsed().as_millis() as u64;

        // Send polling command
        if let Err(e) = writer.write_all(&polling_cmd).and_then(|_| writer.flush()) {
            used_pins[ch_idx].set_low();
            let _ = tx_to_logger.send(vec![4u8]).await;
            terminal_failure = Some(Failure::PollerCommandNotSent(e.to_string()));
            break
        }

        let buff = match read_with_control(
            &mut reader,
            rx_from_server,
            tx_to_server,
            &table_name,
            &interval.to_string(),
            Duration::from_secs_f64(interval.max(0.001) + 5.0)
        ).await {
            ControlledRead::Data(buff) => buff,
            ControlledRead::Stop => {
                stop_requested = true;
                used_pins[ch_idx].set_low();
                let _ = tx_to_logger.send(vec![4u8]).await;
                break
            },
            ControlledRead::Error(e) => {
                used_pins[ch_idx].set_low();
                let _ = tx_to_logger.send(vec![4u8]).await;
                terminal_failure = Some(Failure::MachineNotRespond(e));
                break
            },
        };

        // end-measurement time
        let end_meas_time = start_time.elapsed().as_millis() as u64;

        // Send value to logger
        if buff.len() >= 2 {
            let mut data_vec = start_meas_time.to_ne_bytes().to_vec();
            data_vec.extend_from_slice(&end_meas_time.to_ne_bytes());
            data_vec.extend_from_slice(&ch.to_ne_bytes());
            data_vec.extend_from_slice(&buff);
            if let Err(e) = tx_to_logger.send(data_vec).await {
                println!("Failed to send {}", e)
            };
        }

        // Change the channel
        used_pins[ch_idx].set_low();
        channel_index = (channel_index + 1) % channel_count;

    }

    for pin in used_pins.iter_mut() {
        pin.set_low();
    }

    finish_measurement(
        log_handle,
        terminal_failure,
        tx_to_server,
        rx_from_server,
        stop_requested
    ).await;
}
