use std::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

fn handle_client(stream: TcpStream, tx: mpsc::Sender<Vec<u8>>) {
    println!("{:?}", stream);
}

pub async fn serve(port: u16, tx: mpsc::Sender<Vec<u8>>) -> std::io::Result<()> {

    let listener = TcpListener::bind(format!("127.0.0.1:{}", port))?;

    // accept connections and process them serially
    for stream in listener.incoming() {
        handle_client(stream?);
    }
    
    Ok(())

}