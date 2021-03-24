use crate::{
    protocol::{message::Message, payload::Version},
    setup::{config::NodeConfig, node::Node},
};

use tokio::net::{TcpListener, TcpStream};

use std::collections::HashSet;

#[tokio::test]
async fn handshake_responder_side() {
    // 1. Configure and run node.
    // 2. Send a Version message to the node.
    // 3. Expect a Version back and send Verack.
    // 4. Expect Verack back.

    let config = NodeConfig {
        local_addr: "0.0.0.0:8080".parse().unwrap(),
        ..Default::default()
    };
    let mut node = Node::new(config);
    node.start().await;

    let mut peer_stream = TcpStream::connect(node.local_addr()).await.unwrap();

    Message::Version(Version::new(
        node.local_addr(),
        peer_stream.local_addr().unwrap(),
    ))
    .write_to_stream(&mut peer_stream)
    .await
    .unwrap();

    let version = Message::read_from_stream(&mut peer_stream).await.unwrap();
    assert!(matches!(version, Message::Version(..)));

    Message::Verack
        .write_to_stream(&mut peer_stream)
        .await
        .unwrap();

    let verack = Message::read_from_stream(&mut peer_stream).await.unwrap();
    assert!(matches!(verack, Message::Verack));

    node.stop().await;
}

#[tokio::test]
#[ignore]
async fn handshake_initiator_side() {
    // This needs to be 0.0.0.0 as inbound connection from docker container goes through the
    // machine's IP (duct-tapey but good enough for now).
    let listener = TcpListener::bind("0.0.0.0:8081").await.unwrap();

    // Start node and bind to the listener port.
    let mut initial_peers = HashSet::new();
    initial_peers.insert(
        format!("192.168.64.1:{}", listener.local_addr().unwrap().port())
            .parse()
            .unwrap(),
    );

    let config = NodeConfig {
        local_addr: "0.0.0.0:8080".parse().unwrap(),
        initial_peers,
        ..Default::default()
    };
    let mut node = Node::new(config);
    node.start().await;

    match listener.accept().await {
        Ok((mut peer_stream, addr)) => {
            let version = Message::read_from_stream(&mut peer_stream).await.unwrap();
            assert!(matches!(version, Message::Version(..)));

            Message::Version(Version::new(addr, listener.local_addr().unwrap()))
                .write_to_stream(&mut peer_stream)
                .await
                .unwrap();

            let verack = Message::read_from_stream(&mut peer_stream).await.unwrap();
            assert!(matches!(verack, Message::Verack));

            Message::Verack
                .write_to_stream(&mut peer_stream)
                .await
                .unwrap();
        }
        Err(e) => println!("couldn't get client: {:?}", e),
    }

    node.stop().await;
}
