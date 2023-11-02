#[cfg(test)]
mod tests {
    use tokio::io::{AsyncWriteExt, AsyncReadExt};

    use crate::net::{connect_to_addr, send_packet, read_packet};

	#[tokio::test]
	async fn send_recv_simple() {
		let localhost = "127.0.0.1:1237";
		let data = "Hello";
		let listener = tokio::net::TcpListener::bind(localhost).await.expect("Failed to bind address");
		let accept = listener.accept();
		let connect = connect_to_addr(localhost);
		let (server, client) = futures::future::join(accept, connect).await;
		let (mut server, _) = server.expect("Failed to create listener socket.");
		let mut client = client.expect("Failed to connect client.");
		
		let mut buffer = [0u8; 5];
		server.write_all(data.as_bytes()).await.expect("Failed to send data.");
		client.read(&mut buffer).await.expect("Failed to recieve data.");
	}

	#[tokio::test]
	async fn send_recv() {
		use crate::read_packet;
		let localhost = "127.0.0.1:1238";
		type Data = i32;
		let data: Data = 69420;
		let listener = tokio::net::TcpListener::bind(localhost).await.expect("Failed to bind address");
		let accept = listener.accept();
		let connect = connect_to_addr(localhost);
		let (server, client) = futures::future::join(accept, connect).await;
		let (mut server, _) = server.expect("Failed to create listener socket.");
		let mut client = client.expect("Failed to connect client.");
		send_packet(&mut server, &data).await.expect("Failed to send data.");
		
		let client_data = unsafe { read_packet!(Data, &mut client).await.unwrap() };
		assert_eq!(client_data, data);
	}
}