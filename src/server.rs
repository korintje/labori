use std::net::{TcpListener, TcpStream};
use std::io::{BufReader, Write, Read, BufWriter};
use encoding::{Encoding, EncoderTrap, DecoderTrap};
use encoding::all::ASCII;
use tokio::sync::mpsc;
use crate::error::LaboriError;
use crate::config::Config;
use crate::model::{Command, Response};
use serde_json;

pub async fn serve(
    config: Config,
    tx: mpsc::Sender<Command>,
    rx: mpsc::Receiver<Response>,
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
        let n = reader.read(&mut buff).expect("API RECEIVE FAILURE!!!");
        let request = ASCII.decode(&buff[0..n], DecoderTrap::Replace).unwrap();
        let command: Command = serde_json::from_str(&request).unwrap();
        tx.send(command).await;
    }    

}