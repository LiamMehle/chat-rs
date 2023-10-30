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
			Ok(x) => break Ok(x),
			Err(e) if retry_predicate(&e) => continue,
			Err(e) => break Err(e)
		}
	}
}

async fn async_retry_if<Fut, T, E>(task: impl Fn()->Fut, retry_predicate: impl Fn(&E)->bool) -> Result<T, E>
where Fut: futures::Future<Output=Result<T, E>> {
	loop {
		match task().await {
			Ok(x) => break Ok(x),
			Err(e) if retry_predicate(&e) => continue,
			Err(e) => break Err(e)
		}
	}
}

async fn send_data(address: &str, payload: &[u8]) -> Result<usize, std::io::Error> {
	use tokio::io::AsyncWriteExt;
	use tokio::io::ErrorKind::{WouldBlock, ConnectionRefused, NotConnected};
	let _ = OnDrop::new(||{println!("send_data exited")});
	let should_retry_on = |e: &tokio::io::Error| { [WouldBlock, ConnectionRefused, NotConnected].contains(&e.kind()) };
	let mut sender = async_retry_if(||{TcpStream::connect(address)}, should_retry_on).await?;
	sender.ready(Interest::WRITABLE).await?;
	// sender is guaranteed to be ready for a write op
	sender.write_all(payload).await.map(|_| payload.len())
}

// retries on WOULDBLOCK
async fn async_read<T: AsMut<[u8]>>(stream: TcpStream, receive_buffer: &mut T) -> Result<usize, tokio::io::Error> {
	use tokio::io::ErrorKind::WouldBlock;
	let should_retry_on = |e: &tokio::io::Error| { e.kind() == WouldBlock };
	retry_if(||{stream.try_read(&mut receive_buffer.as_mut())}, should_retry_on)
}

async fn async_read_exact<T: AsMut<[u8]>>(stream: TcpStream, receive_buffer: &mut T) -> Result<usize, tokio::io::Error> {
	use tokio::io;
	let mut buffer_free_slice = receive_buffer.as_mut();
	loop {
		stream.ready(Interest::READABLE).await?;
		match stream.try_read(&mut buffer_free_slice.as_mut()) {
			Ok(n) if n == buffer_free_slice.len()           => break Ok(n),
			Ok(n)                                           => buffer_free_slice = buffer_free_slice[n..].as_mut(),
			Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
			Err(e)                                          => break Err(e)
		}
	}
}

async fn receive_data(address: &str, expected_len: usize) -> Result<String, std::io::Error> {
	let listener = TcpListener::bind(address).await?;
	println!("listener created");
	let (stream, _) = listener.accept().await?;
	// assert_eq!(sender_address.to_owned().to_string().as_str(), address);
	let mut receive_buffer = Vec::with_capacity(expected_len);
	receive_buffer.resize(expected_len, 0);
	let receive_length = async_read_exact(stream, &mut receive_buffer).await?;
	String::from_utf8(receive_buffer[..receive_length].to_vec()).map_err(|error| Error::new(std::io::ErrorKind::Other, error))
}

#[tokio::main]
async fn main() {
	let localhost = "127.0.0.1:1239";
	let payload = "Hello, remote world!".as_bytes();
	let send_task = send_data(localhost, payload);
	let receive_task = receive_data(localhost, payload.len());
	let (received_payload, bytes_sent) = futures::future::join(send_task, receive_task).await;
	println!("[{:?}]: {:?}", bytes_sent, received_payload);
}
