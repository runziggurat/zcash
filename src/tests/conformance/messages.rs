use crate::{
    helpers::{handshake, handshaken_peer},
    protocol::{
        message::{Message, MessageFilter},
        payload::Nonce,
    },
    setup::{config::read_config_file, node::Node},
    wait_until,
};

use tokio::{
    net::TcpListener,
    time::{timeout, Duration},
};

#[tokio::test]
async fn ping_pong() {
    let (zig, node_meta) = read_config_file();

    let listener = TcpListener::bind(zig.new_local_addr()).await.unwrap();

    // Create a node and set the listener as an initial peer.
    let mut node = Node::new(node_meta);
    node.initial_peers(vec![listener.local_addr().unwrap().port()])
        .start()
        .await;

    // Receive the connection and perform the handshake once the node is started.
    let mut peer_stream = handshake(listener).await.unwrap();

    Message::Ping(Nonce::default())
        .write_to_stream(&mut peer_stream)
        .await
        .unwrap();

    let auto_responder = MessageFilter::with_all_auto_reply().enable_logging();

    wait_until!(
        10,
        matches!(
            auto_responder
                .read_from_stream(&mut peer_stream)
                .await
                .unwrap(),
            Message::Pong(..)
        )
    );

    node.stop().await;
}

#[tokio::test]
#[ignore]
async fn unsolicitation_listener() {
    let (_zig, node_meta) = read_config_file();

    let mut node = Node::new(node_meta);
    node.start().await;

    let mut peer_stream = handshaken_peer(node.addr()).await.unwrap();

    let auto_responder = MessageFilter::with_all_auto_reply().enable_logging();

    for _ in 0usize..10 {
        let result = timeout(
            Duration::from_secs(5),
            auto_responder.read_from_stream(&mut peer_stream),
        )
        .await;

        match result {
            Err(elapsed) => println!("Timeout after {}", elapsed),
            Ok(Ok(message)) => println!("Received unfiltered message: {:?}", message),
            Ok(Err(err)) => println!("Error receiving message: {:?}", err),
        }
    }

    node.stop().await;
}
