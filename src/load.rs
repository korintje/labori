use crate::{error, model};
use tokio::sync::mpsc;
use std::net::TcpStream;
use std::io::{BufReader, Write, Read, BufWriter};
use encoding::{Encoding, EncoderTrap};
use encoding::all::ASCII;
use tokio::time::{sleep, Duration};

pub async fn get_data_tcp(tx0: mpsc::Sender<Vec<u8>>) -> Result<(), error::LaboriError> {

    // Prepare command bytes
    let trigger_cmd = ":LOG:LEN 5e5; :LOG:CLE; :FUNC FINA; :GATE:TIME 0.1; :FRUN ON\n";
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
        sleep(Duration::from_millis(1000)).await;

        // Data polling loop
        loop {

            writer.write(&polling_cmd).expect("Polling FAILURE!!!");
            writer.flush().unwrap();

            let mut buff = vec![0; 1024];
            let n = reader.read(&mut buff).expect("RECEIVE FAILURE!!!");
            // println!("{:?}", &buff[..n]);
            println!("{}", n);
            // println!("{:?}", &buff[0..n]);
            
            if n >= 2 {
                // println!("{:?}", &buff[..n]);
                if let Err(e) = tx0.send(buff[..n].to_vec()).await {
                    panic!("Failed to send {}", e)
                };
                
            }
            
            sleep(Duration::from_millis(10000)).await;
            // println!("100 ms have elapsed");      
        }

    } else {
        println!("Couldn't connect to server...");
    }

    println!("finished");
    Ok(())

  }
