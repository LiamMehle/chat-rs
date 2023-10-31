use tokio::net::{TcpListener, TcpStream};
use tokio::io::{Result, TcpStream, Interest, Error};

pub async fn connect_to_addr(address: &str) -> Result<TcpStream> {
    let should_retry_on = |e: &Error| { [WouldBlock, ConnectionRefused, NotConnected].contains(&e.kind()) };
	let connect = ||{TcpStream::connect(address)};
    async_retry_if(connect, should_retry_on).await
}