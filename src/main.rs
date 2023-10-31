use std::io::Error;

use tokio::net::{TcpListener, TcpStream};
use tokio::io::Interest;

struct OnDrop<F: Copy + FnOnce() -> ()> {
	closure: F
}

impl<F: Copy + FnOnce() -> ()> OnDrop<F> {
	fn new(closure: F) -> Self {
		Self { closure: closure }
	}
}

impl<F: Copy + FnOnce() -> ()> Drop for OnDrop<F> {
	fn drop(&mut self) {
		(self.closure)()
	}
}

fn retry_if<T, E>(mut task: impl FnMut()->Result<T, E>, retry_predicate: impl Fn(&E)->bool) -> Result<T, E> {
	loop {
		match task() {
			Err(e) if retry_predicate(&e) => continue,
			x => break x
		}
	}
}

async fn async_retry_if<Fut, T, E>(task: impl Fn()->Fut, retry_predicate: impl Fn(&E)->bool) -> Result<T, E>
where Fut: futures::Future<Output=Result<T, E>> {
	loop {
		match task().await {
			Err(e) if retry_predicate(&e) => tokio::task::yield_now().await,
			x => break x
		}
	}
}

async fn send_data(address: &str, payload: &[u8]) -> Result<usize, std::io::Error> {
	use tokio::io::AsyncWriteExt;
	use tokio::io::ErrorKind::{WouldBlock, ConnectionRefused, NotConnected};
	let _ = OnDrop::new(||{println!("send_data exited")});
	let should_retry_on = |e: &tokio::io::Error| { [WouldBlock, ConnectionRefused, NotConnected].contains(&e.kind()) };
	let connect = ||{TcpStream::connect(address)};
	let mut sender = async_retry_if(connect, should_retry_on).await?;
	sender.ready(Interest::WRITABLE).await?;
	// sender is guaranteed to be ready for a write op
	sender.write_all(payload).await.map(|_| payload.len())
}

// retries on WOULDBLOCK
async fn async_read<T: AsMut<[u8]>>(stream: TcpStream, receive_buffer: &mut T) -> Result<usize, tokio::io::Error> {
	use tokio::io::ErrorKind::WouldBlock;
	let should_retry_on = |e: &tokio::io::Error| { e.kind() == WouldBlock };
	let read_from_stream = ||{stream.try_read(&mut receive_buffer.as_mut())};
	retry_if(read_from_stream, should_retry_on)
}

async fn receive_data(address: &str, expected_len: usize) -> Result<String, std::io::Error> {
	let listener = TcpListener::bind(address).await?;
	println!("listener created");
	let (stream, _) = listener.accept().await?;
	// assert_eq!(sender_address.to_owned().to_string().as_str(), address);
	let mut receive_buffer = Vec::with_capacity(expected_len);
	receive_buffer.resize(expected_len, 0);
	stream.ready(Interest::READABLE).await?;
	let receive_length = async_read(stream, &mut receive_buffer).await?;
	String::from_utf8(receive_buffer[..receive_length].to_vec()).map_err(|error| Error::new(std::io::ErrorKind::Other, error))
}

#[tokio::main]
async fn main() {
	
}

#[cfg(test)]
mod tests {
	#[tokio::test]
	async fn socket_connection() {
		use crate::*;
		let localhost = "127.0.0.1:1239";
		let payload = "Hello, remote world!".as_bytes();
		let send_task = send_data(localhost, payload);
		let receive_task = receive_data(localhost, payload.len());
		let (received_payload, bytes_sent) = futures::future::join(send_task, receive_task).await;
		assert!(bytes_sent.is_ok());
		assert!(received_payload.is_ok());
		let bytes_sent_count = bytes_sent.unwrap().len();
		let received_payload_len = received_payload.unwrap();
		assert_eq!(received_payload_len, bytes_sent_count);
		assert_eq!(payload.len(), bytes_sent_count);
	}
	#[tokio::test]
	async fn socket_address_in_use() {
		use crate::*;
		let localhost = "127.0.0.1:1239";
		let payload = "Hello, remote world!".as_bytes();
		let send_task = send_data(localhost, payload);
		let send_task2 = send_data(localhost, payload);
		let (send_result, send_result2) = futures::future::join(send_task, send_task2).await;
		assert!(send_result.is_err() || send_result2.is_err());
		assert!(send_result.unwrap_err().kind() == tokio::io::ErrorKind::AddrInUse || send_result2.unwrap_err().kind() == tokio::io::ErrorKind::AddrInUse);
	}
}