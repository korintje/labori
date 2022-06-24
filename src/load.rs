use tokio::io::{self, AsyncWriteExt, AsyncReadExt};
use tokio::{sync::mpsc};
use crate::{error};

use tokio::net::TcpStream;
use encoding::{Encoding, EncoderTrap};
use encoding::all::ASCII;


use tokio::net::TcpListener;
use tokio_stream::StreamExt;
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

pub async fn get_data_tcp(tx0: mpsc::Sender<Vec<u8>>) -> Result<(), error::SQLMDError> {
    
    let cmd = ":LOG:LEN 5e5; :LOG:CLE; :FUNC FINA; :GATE:TIME 0.1; :FRUN ON\n";
    let encoded = ASCII.encode(cmd, EncoderTrap::Replace).unwrap();

    let listener = TcpListener::bind("192.168.200.44:5198").await.unwrap();
    let (client, _) = listener.accept().await.unwrap();
    let frame_writer = FramedWrite::new(client, LengthDelimitedCodec::new());

    loop {
        let (client, _) = listener.accept().await.unwrap();
        let mut frame_reader = FramedRead::new(client, LengthDelimitedCodec::new());
        while let Some(frame) = frame_reader.next().await {
            match frame {
                Ok(data) => println!("received: {:?}", data),
                Err(err) => eprintln!("error: {:?}", err),
            }
        }
    }

}

pub async fn rget_data_tcp(tx0: mpsc::Sender<Vec<u8>>) -> Result<(), error::SQLMDError> {

    // Connect to a peer
    let socket = TcpStream::connect("192.168.200.44:5198").await?;
    let (mut rd, mut wr) = io::split(socket);

    // 
    let cmd = ":LOG:LEN 5e5; :LOG:CLE; :FUNC FINA; :GATE:TIME 0.1; :FRUN ON\n";
    let encoded = ASCII.encode(cmd, EncoderTrap::Replace).unwrap();
    wr.write_all(&encoded[..]).await?;

    // Prepare read buffer
    let mut buffer = vec![0; 1025];

    loop {

        let query = ":LOG:DATA?\n";
        let encoded = ASCII.encode(query, EncoderTrap::Replace).unwrap();
        wr.write_all(&encoded[..]).await?;
        // println!("{}", "requested.\r");

        let n = rd.read(&mut buffer).await?;
        if n > 2 {
            println!("received: {:?}", &buffer[..n]);
        }
        // println!("{}", "read.\r");

        match tx0.send(buffer[..n].to_vec()).await {
            Ok(_) => (),
            Err(e) => {
                println!("TX0 send error: {}", e);
                // error::SQLMDError::ParseIntError(e);
                break
            }
        };
        // println!("{}", "send.");
    }

    Ok(())

}