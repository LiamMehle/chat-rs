use core::slice;
use std::io::ErrorKind;

use serde::{Serialize, Deserialize};
use futures::TryFutureExt;
use tokio::net::TcpStream;
use tokio::io::{Result, Interest, Error, ErrorKind::{WouldBlock, ConnectionRefused, NotConnected}};

type PacketId = u8;

#[derive(Serialize, Deserialize, PartialEq, PartialOrd, Debug, Clone)]
struct Login {
	pub username: String,
	pub hashed_password: String
}

#[derive(Serialize, Deserialize, PartialEq, PartialOrd, Debug, Clone)]
struct Message {
	pub from: String,
	pub to: String,
	pub message: String
}

#[derive(Serialize, Deserialize, PartialEq, PartialOrd, Debug, Clone)]
enum Action {
	Login(Login),
	Message(Message),
	MessageLogsRequest,
	MessageLogs
}

#[allow(dead_code)]
pub async fn connect_to_addr(address: &str) -> Result<TcpStream> {
	use crate::combinator::async_retry_if;

	let should_retry_on = |e: &Error| { [WouldBlock, ConnectionRefused, NotConnected].contains(&e.kind()) };
	let connect = ||{TcpStream::connect(address)};

	async_retry_if(connect, should_retry_on).await
}

fn into_packet<'a, T>(data: &'a T) -> Vec<u8> {
	use std::io::Read;
	let len_len = std::mem::size_of::<u32>();
	let data_len = std::mem::size_of::<T>();
	let output_len = len_len + data_len;
	let mut output = Vec::<u8>::with_capacity(output_len);
	unsafe { output.set_len(output_len) };
	let mut len_slice = unsafe { std::slice::from_raw_parts(&data_len as *const _ as *const u8, len_len) };
	let mut data_slice = unsafe { std::slice::from_raw_parts(data as *const T as *const u8, data_len) };
	let _ = len_slice.read_exact(output[..len_len].as_mut());
	let _ = data_slice.read_exact(output[len_len..].as_mut());

	output
}

async fn await_writable(connection: &TcpStream) -> tokio::io::Result<tokio::io::Ready> {
	loop {
		match connection.ready(Interest::WRITABLE).await {
		Ok(ready) if !ready.is_writable() => tokio::task::yield_now().await,
		x => break x
		}
	}
}
async fn await_readable(connection: &TcpStream) -> tokio::io::Result<tokio::io::Ready> {
	loop {
		match connection.ready(Interest::READABLE).await {
		Ok(ready) if !ready.is_readable() => tokio::task::yield_now().await,
		x => break x
		}
	}
}

#[allow(dead_code)]
pub async fn send_packet<T>(connection: &mut TcpStream, data: &T) -> Result<()> {
	use tokio::io::AsyncWriteExt;

	let packet = into_packet(data);
	let _ = await_writable(&connection).await;
	connection.write_all(packet.as_ref()).await
}

#[allow(dead_code)]
pub async unsafe fn read_packet<T: Sized, const T_SIZE: usize>(connection: &mut TcpStream) -> Result<T> {
	use tokio::io::AsyncReadExt;

	const LEN_LEN: usize = std::mem::size_of::<u32>();
	assert_eq!(std::mem::size_of::<T>(), T_SIZE);
	
	let _ = await_readable(&connection).await?;
	let mut packet_len = 0u32;
	let packet_len_slice = slice::from_raw_parts_mut(&mut packet_len as *mut _ as *mut u8, std::mem::size_of::<u32>());
	connection.read(packet_len_slice).await?;
	if packet_len as usize != std::mem::size_of::<T>() {
		println!("packet header shows size of {}, but expected type is of size {}", packet_len, std::mem::size_of::<T>());
		return Err(Error::new(ErrorKind::InvalidData, "wrong type expected"));
	}
	let mut known_size_buffer =  [0u8; T_SIZE];
	let _ = await_readable(&connection).await?;
	let read_size = connection.read(&mut known_size_buffer).await?;
	assert_eq!(read_size, T_SIZE);

	let mut output: T = std::mem::zeroed();
	let output_slice = slice::from_raw_parts_mut(&mut output as *mut _ as *mut u8, T_SIZE);
	known_size_buffer.as_ref().read_exact(output_slice).map_err(|e| tokio::io::Error::from(e)).await?;
	Ok(output)
}

#[allow(unused_macros)]
#[macro_export]
macro_rules! read_packet {
	($t:ty, $c:expr) => { read_packet::< $t, {std::mem::size_of::<$t>()} >($c) }
}


#[cfg(test)]
pub mod tests {
	#[test]
	fn login_packet() {
		use crate::net::{Action, Login};
		use serde_json;
		let action = Action::Login(Login {
				username: "me".to_owned(),
				hashed_password: "my pass".to_owned()
			});
		// let serialized_action = serde_json::to_string(&action).unwrap();
	}
}