use core::slice;
use std::io::ErrorKind;
use std::mem;

use futures::TryFutureExt;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio::io::{Result, Interest, Error, ErrorKind::{WouldBlock, ConnectionRefused, NotConnected}};

#[allow(dead_code)]
pub async fn connect_to_addr(address: &str) -> Result<TcpStream> {
	use crate::combinator::async_retry_if;

	let should_retry_on = |e: &Error| { [WouldBlock, ConnectionRefused, NotConnected].contains(&e.kind()) };
	let connect = ||{TcpStream::connect(address)};

	async_retry_if(connect, should_retry_on).await
}

fn into_packet<'a, T>(data: &'a T) -> (usize, impl Iterator<Item=&'a u8>) {
	let data_digest = unsafe { slice::from_raw_parts(&data as *const _ as *const u8, std::mem::size_of::<T>()) };
	let data_len: usize = data_digest.len();
	let data_len_digest: &[u8] = unsafe { slice::from_raw_parts(&data_len as *const _ as *const u8, std::mem::size_of::<usize>()) };

	(data_len, data_len_digest.into_iter().chain(data_digest))
}

fn into_packet_vec<T>(data: &T) -> Vec<u8> {
	let (len, data) = into_packet(data);
	let mut vec = Vec::with_capacity(len);
	vec.extend(data);

	vec
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

	let packet = into_packet_vec(data);
	let _ = await_writable(&connection).await;
	connection.write_all(packet.as_ref()).await
}

#[allow(dead_code)]
pub async unsafe fn read_packet<T: Sized, const T_SIZE: usize>(connection: &mut TcpStream) -> Result<T> {
	const USIZE_LEN: usize = std::mem::size_of::<usize>();
	assert_eq!(std::mem::size_of::<T>(), T_SIZE);
	
	let _ = await_readable(&connection).await?;
	let mut len_buffer = [0u8; USIZE_LEN];
	connection.read(&mut len_buffer).await?;
	let len: usize = std::mem::transmute(len_buffer);
	if len != std::mem::size_of::<T>() {
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
