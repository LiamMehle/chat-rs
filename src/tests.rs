#[cfg(test)]
mod tests {
    use crate::net::{connect_to_addr, send_packet, read_packet};

	#[tokio::test]
	async fn send_recv() {
		use crate::read_packet;
		let localhost = "127.0.0.1:12346";
		type Data = i32;
		let data: Data = 69420;
		let listener = tokio::net::TcpListener::bind(localhost).await.expect("Failed to bind address");
		let accept = listener.accept();
		let connect = connect_to_addr(localhost);
		let (server, client) = futures::future::join(accept, connect).await;
		let (mut server, _) = server.expect("Failed to create listener socket.");
		let mut client = client.expect("Failed to connect client.");
		send_packet(&mut server, &data).await.expect("Failed to send data.");
		unsafe { read_packet!(Data, &mut client).await.unwrap() };
	}
}