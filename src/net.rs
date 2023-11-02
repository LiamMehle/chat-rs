use std::mem;

use tokio::net::{TcpListener, TcpStream};
use tokio::io::{Result, Interest, Error, ErrorKind::{WouldBlock, ConnectionRefused, NotConnected}};

#[allow(dead_code)]
pub async fn connect_to_addr(address: &str) -> Result<TcpStream> {
	use crate::combinator::async_retry_if;

	let should_retry_on = |e: &Error| { [WouldBlock, ConnectionRefused, NotConnected].contains(&e.kind()) };
	let connect = ||{TcpStream::connect(address)};

	async_retry_if(connect, should_retry_on).await
}

fn into_packet<'a, T>(data: &'a T) -> (usize, impl Iterator<Item=&'a u8>) {
	let data_digest: &[u8] = unsafe { mem::transmute_copy(data) };
	let data_len: usize = data_digest.len();
	let data_len_digest: &[u8] = unsafe { mem::transmute_copy(&data_len) };

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

#[allow(dead_code)]
pub async fn send_packet<T>(connection: &mut TcpStream, data: &T) -> Result<()> {
	use tokio::io::AsyncWriteExt;

	let packet = into_packet_vec(data);
	let _ = await_writable(&connection).await;
	connection.write_all(packet.as_ref()).await
}