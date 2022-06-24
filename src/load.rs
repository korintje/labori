use tokio::io::{self, AsyncWriteExt, AsyncReadExt};
use tokio::{sync::mpsc};
use crate::{error};

use tokio::net::TcpStream;
use encoding::{Encoding, EncoderTrap};
use encoding::all::ASCII;



pub async fn get_data_tcp(tx0: mpsc::Sender<Vec<u8>>) -> Result<(), error::SQLMDError> {

    // Connect to a peer
    let socket = TcpStream::connect("192.168.200.44:5198").await?;
    let (mut rd, mut wr) = io::split(socket);

    // 
    let cmd = ":LOG:LEN 5e5; :LOG:CLE; :FUNC FINA; :GATE:TIME 1; :FRUN ON\n";
    let encoded = ASCII.encode(cmd, EncoderTrap::Replace).unwrap();
    wr.write_all(&encoded[..]).await?;

    // Prepare read buffer
    let mut buffer = vec![0; 1024];

    loop {

        let query = ":LOG:DATA?\n";
        let encoded = ASCII.encode(query, EncoderTrap::Replace).unwrap();
        wr.write_all(&encoded[..]).await?;

        let n = rd.read(&mut buffer).await?;
        match tx0.send(buffer[..n].to_vec()).await {
            Ok(_) => (),
            Err(e) => {
                println!("TX0 send error: {}", e);
                break
            }
        };
    }

    Ok(())

}