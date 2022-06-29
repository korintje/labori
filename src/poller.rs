/*
use crate::model::{Command, Response, Success, Failure};
use crate::config::Config;
use crate::error::LaboriError;
use crate::logger;
use std::net::TcpStream;
use tokio::sync::mpsc;
use std::io::{BufReader, Write, Read, BufWriter};
use encoding::{Encoding, EncoderTrap, DecoderTrap};
use encoding::all::ASCII;
use tokio::time::{sleep, Duration};
use chrono::Local;



async fn poll(
  device_name: String,
  stream: &TcpStream,
  tx_to_client: &mpsc::Sender<Response>,
  tx_to_logger: &mpsc::Sender<Vec<u8>>,
  rx_from_client: &mut mpsc::Receiver<Command>,
) -> Result<(), LaboriError> {

  // Prepare command bytes
  let polling_cmd = ":LOG:DATA?\n";
  let polling_cmd = ASCII.encode(polling_cmd, EncoderTrap::Replace).unwrap();

  // Prepare buffers
  let mut reader = BufReader::new(stream);
  let mut writer = BufWriter::new(stream);

  // Spawn logger
  
  let (tx_to_logger, rx_from_client) = mpsc::channel(1024);
  
  let log_handle = tokio::spawn(
      logger::log(device_name.clone(), table_name.to_string(), rx_from_client)
  );

  // Data polling loop
  loop {

      writer.write(&polling_cmd).expect("Polling FAILURE!!!");
      writer.flush().unwrap();

      let mut buff = vec![0; 1024];
      let n = reader.read(&mut buff).expect("RECEIVE FAILURE!!!");
      println!("{}\r", n);
      
      if n >= 2 {
          if let Err(e) = tx_to_logger.send(buff[..n].to_vec()).await {
              println!("Failed to send {}", e)
          };
      }
      
      // Controll polling interval
      // check if kill signal has been sent
      match rx_from_server.try_recv() {
          Ok(cmd) => {
              match cmd {
                  Command::Stop {} => {
                      if let Err(e) = tx_to_logger.send(vec![4u8]).await {
                          println!("Failed to send kill signal {}", e)
                      };
                      break
                  },
                  _ => tx_to_server.send(
                      Response::Failed(Failed::Busy)
                  ).await.unwrap()
              } 
          },
          Err(_) => (),
      }
      sleep(Duration::from_millis(10)).await  
  }

  if let Err(e) = log_handle.await.unwrap() {
    return Err(LaboriError::LogError(e.to_string()))
  }

  Ok(())

}
*/