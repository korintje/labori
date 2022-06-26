use crate::{error};
use tokio::sync::mpsc;
use std::net::TcpStream;
use std::io::{BufReader, Write, Read, BufWriter};
use encoding::{Encoding, EncoderTrap};
use encoding::all::ASCII;

pub async fn get_data_tcp(tx0: mpsc::Sender<Vec<u8>>) -> Result<(), error::SQLMDError> {

    // Prepare command bytes
    let trigger_cmd = ":LOG:LEN 5e5; :LOG:CLE; :FUNC FINA; :GATE:TIME 0.01; :FRUN ON\n";
    let trigger_cmd = ASCII.encode(trigger_cmd, EncoderTrap::Replace).unwrap();
    let polling_cmd = ":LOG:DATA?\n";
    let polling_cmd = ASCII.encode(polling_cmd, EncoderTrap::Replace).unwrap();

    let addr = "192.168.200.44:5198";
    if let Ok(stream) = TcpStream::connect(addr) {

        println!("Connection Ok.");

        // Prepare buffers
        let mut reader = BufReader::new(&stream);
        let mut writer = BufWriter::new(&stream);

        // Trigger measurements
        writer.write(&trigger_cmd).expect("Trigger FAILURE!!!");
        writer.flush().unwrap();

        // Data polling loop
        loop {

            writer.write(&polling_cmd).expect("Polling FAILURE!!!");
            writer.flush().unwrap();

            let mut buff = vec![0; 1025];
            let n = reader.read(&mut buff).expect("RECEIVE FAILURE!!!");
            if n >= 2 {
                match tx0.send(buff[..n].to_vec()).await {
                    Ok(_) => println!("send ok"),
                    Err(e) => panic!("Failed to send {}", e),
                };
            }
            // println!("{:?}", &buff[..n]);
        }

    }

    Ok(())

  }
