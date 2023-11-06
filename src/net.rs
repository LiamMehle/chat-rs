use core::slice;
use std::io::ErrorKind;

use futures::TryFutureExt;
use tokio::net::TcpStream;
use tokio::io::{Result, Interest, Error, ErrorKind::{WouldBlock, ConnectionRefused, NotConnected}};

type PacketId = u8;

trait Serialize {
	fn seriazlie(&self) -> Vec<u8>;
}

trait Deserialize where Self: Sized {
	fn deseriazlie(buffer: impl AsRef<[u8]>) -> Option<Self>;
}

#[derive(PartialEq, PartialOrd, Debug, Clone)]
struct Login {
	pub username: String,
	pub hashed_password: String
}

#[derive(PartialEq, PartialOrd, Debug, Clone)]
struct Message {
	pub from: String,
	pub to: String,
	pub message: String
}

impl Serialize for Login {
	fn seriazlie(&self) -> Vec<u8> {
		const SEP: u8 = '|' as u8;
		const SEP_LEN: usize = 1;

		let mut output = Vec::with_capacity(self.username.len()+SEP_LEN+self.hashed_password.len());
		output.push(LOGIN_DISCRIMINANT);
		output.extend_from_slice(self.username.as_bytes());
		output.push(SEP);
		output.extend_from_slice(self.hashed_password.as_bytes());

		output
	}
}

impl Deserialize for Login {
	fn deseriazlie(buffer: impl AsRef<[u8]>) -> Option<Self> {
		let mut parts = buffer.as_ref().split(|c| *c == '|' as u8);
		let raw_username = parts.next()?;
		let raw_password = parts.next()?;
		let username = String::from_utf8(raw_username.to_owned()).ok()?;
		let password = String::from_utf8(raw_password.to_owned()).ok()?;
		
		if let Some(_) = parts.next() { return None }

		Some(Login {
			username: username,
			hashed_password: password
		})
	}
}

const LOGIN_DISCRIMINANT: PacketId = 1;
const MESSAGE_DISCRIMINANT: PacketId = 2;
const MESSAGE_LOGS_REQUEST_DISCRIMINANT: PacketId = 3;
const MESSAGE_LOGS_DISCRIMINANT: PacketId = 4;

#[derive(PartialEq, PartialOrd, Debug, Clone)]
enum Action {
	Login(Login),
	Message(Message),
	MessageLogsRequest,
	MessageLogs
}

impl Serialize for Action {
	fn seriazlie(&self) -> Vec<u8> {
		match self {
			Self::Login(x) => x.seriazlie(),
			Self::Message(_) => todo!(),
			Self::MessageLogsRequest => todo!(),
			Self::MessageLogs => todo!()
		}
	}
}

impl Deserialize for Action {
	fn deseriazlie(buffer: impl AsRef<[u8]>) -> Option<Self> {
		const ID_LEN:usize = std::mem::size_of::<PacketId>();
		let buffer = buffer.as_ref();
		// to_be() does the same as a "from_be", if the machine is big endian, it's a no-op, if it's little endian, the bytes are swapped.
		// network order is big-endian
		let variant_id = unsafe { std::mem::transmute::<[u8; ID_LEN], PacketId>(buffer[..ID_LEN].try_into().unwrap()) }.to_be();
		let payload = &buffer[ID_LEN..];

		match variant_id {
			x if x == LOGIN_DISCRIMINANT => Login::deseriazlie(payload).map(|x| Self::Login(x)),
			// x if x == message => {todo!()},
			// x if x == request_message_log => {todo!()},
			// x if x == message_log => {todo!()}
			_ => {todo!()}
		}
	}
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
    use crate::net::{Serialize, Deserialize};
	#[test]
	fn login_packet() {
		use crate::net;
		let action = net::Action::Login(net::Login {username: "1234".to_owned(), hashed_password: "5678".to_owned()});
		let packet_data = action.seriazlie();
		let deserialized_action = net::Action::deseriazlie(packet_data).unwrap();
		assert_eq!(deserialized_action, action);
	}
}