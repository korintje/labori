use crate::error::LaboriError;
use crate::model::Signal;
use crate::config::Config;
use tokio::sync::mpsc;
use std::net::TcpStream;
use std::io::{BufReader, Write, Read, BufWriter};
use encoding::{Encoding, EncoderTrap};
use encoding::all::ASCII;
use tokio::time::{sleep, Duration};

pub async fn run(config: Config, tx0: mpsc::Sender<Vec<u8>>, mut rx1: mpsc::Receiver<Signal>) 
-> Result<(), LaboriError> {

    // Prepare command bytes
    // let trigger_cmd = ":LOG:LEN 5e5; :LOG:CLE; :FUNC FINA; :GATE:TIME 0.1; :FRUN ON\n";
    let trigger_cmd = ":LOG:LEN 5e5; :LOG:CLE; :FRUN ON\n";
    let trigger_cmd = ASCII.encode(trigger_cmd, EncoderTrap::Replace).unwrap();
    let polling_cmd = ":LOG:DATA?\n";
    let polling_cmd = ASCII.encode(polling_cmd, EncoderTrap::Replace).unwrap();

    loop {

        let signal = if let Some(item) = rx1.recv().await {item} else {continue};
        if let Signal::Start = signal {

            if let Ok(stream) = TcpStream::connect(&config.device_addr) {

                println!("Connection Ok.");
    
                // Prepare buffers
                let mut reader = BufReader::new(&stream);
                let mut writer = BufWriter::new(&stream);
    
                // Trigger measurements
                writer.write(&trigger_cmd).expect("Trigger FAILURE!!!");
                writer.flush().unwrap();
                sleep(Duration::from_millis(1000)).await;            

                // Data polling loop
                loop {

                    writer.write(&polling_cmd).expect("Polling FAILURE!!!");
                    writer.flush().unwrap();

                    let mut buff = vec![0; 1024];
                    let n = reader.read(&mut buff).expect("RECEIVE FAILURE!!!");
                    println!("{}", n);
                    
                    if n >= 2 {
                        if let Err(e) = tx0.send(buff[..n].to_vec()).await {
                            println!("Failed to send {}", e)
                        };
                    }
                    
                    // Controll polling interval
                    sleep(Duration::from_millis(10)).await;
                    
                    // Check stop signal
                    if let Some(signal) = rx1.recv().await {
                        if let Signal::Stop = signal {
                            break                        
                        }
                    }
                    
                }
            }
        }
    }
}
