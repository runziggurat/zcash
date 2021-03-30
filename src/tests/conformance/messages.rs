use crate::{
    protocol::{
        message::Message,
        payload::{Nonce, Version},
    },
    setup::{config::read_config_file, node::Node},
};

use tokio::{io, net::TcpStream};

use std::net::SocketAddr;

#[tokio::test]
async fn ping_pong() {
    let (_zig, node_meta) = read_config_file();

    let mut node = Node::new(node_meta);
    node.start().await;

    let mut peer_stream = handshake(node.addr()).await.unwrap();

    Message::Ping(Nonce::default())
        .write_to_stream(&mut peer_stream)
        .await
        .unwrap();

    let pong = Message::read_from_stream(&mut peer_stream).await.unwrap();
    assert!(matches!(pong, Message::Pong(..)));

    node.stop().await;
}

async fn handshake(node_addr: SocketAddr) -> io::Result<TcpStream> {
    let mut peer_stream = TcpStream::connect(node_addr).await?;

    Message::Version(Version::new(node_addr, peer_stream.local_addr().unwrap()))
        .write_to_stream(&mut peer_stream)
        .await
        .unwrap();

    let version = Message::read_from_stream(&mut peer_stream).await?;
    assert!(matches!(version, Message::Version(..)));

    Message::Verack
        .write_to_stream(&mut peer_stream)
        .await
        .unwrap();

    let verack = Message::read_from_stream(&mut peer_stream).await?;
    assert!(matches!(verack, Message::Verack));

    Ok(peer_stream)
}
