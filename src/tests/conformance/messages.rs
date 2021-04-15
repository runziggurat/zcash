use crate::{
    helpers::{initiate_handshake, respond_to_handshake},
    protocol::{
        message::{Message, MessageFilter},
        payload::{block::Headers, Addr, Nonce},
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
    let mut peer_stream = respond_to_handshake(listener).await.unwrap();

    let nonce = Nonce::default();
    Message::Ping(nonce)
        .write_to_stream(&mut peer_stream)
        .await
        .unwrap();

    wait_until!(10, {
        // Ignore queries from the node.
        let auto_responder = MessageFilter::with_all_auto_reply();
        if let Ok(Message::Pong(returned_nonce)) =
            auto_responder.read_from_stream(&mut peer_stream).await
        {
            // We received a pong and the nonce matches.
            assert_eq!(nonce, returned_nonce);
            true
        } else {
            // We didn't receive a pong.
            false
        }
    });

    node.stop().await;
}

#[tokio::test]
async fn ignores_unsolicited_responses() {
    // ZG-CONFORMANCE-009
    //
    // The node ignore certain unsolicited messages but doesn’t disconnect.
    //
    // Messages to be tested: Reject, NotFound, Pong, Tx, Block, Header, Addr.
    //
    // Test procedure:
    //      Complete handshake, and then for each test message:
    //
    //      1. Send the message
    //      2. Send a ping request
    //      3. Receive a pong response

    let (zig, node_meta) = read_config_file();

    let listener = TcpListener::bind(zig.new_local_addr()).await.unwrap();

    // Create a node and set the listener as an initial peer.
    let mut node = Node::new(node_meta);
    node.initial_peers(vec![listener.local_addr().unwrap().port()])
        .start()
        .await;

    let mut stream = crate::helpers::respond_to_handshake(listener)
        .await
        .unwrap();

    // TODO: rest of the message types
    let test_messages = vec![
        Message::Pong(Nonce::default()),
        Message::Headers(Headers::empty()),
        Message::Addr(Addr::empty()),
        // Block(Block),
        // NotFound(Inv),
        // Tx(Tx),
    ];

    let filter = MessageFilter::with_all_auto_reply().enable_logging();

    for message in test_messages {
        message.write_to_stream(&mut stream).await.unwrap();

        let nonce = Nonce::default();
        Message::Ping(nonce)
            .write_to_stream(&mut stream)
            .await
            .unwrap();

        match filter.read_from_stream(&mut stream).await.unwrap() {
            Message::Pong(returned_nonce) => assert_eq!(nonce, returned_nonce),
            msg => panic!("Expected pong: {:?}", msg),
        }
    }

    node.stop().await;
}

#[allow(dead_code)]
async fn unsolicitation_listener() {
    let (_zig, node_meta) = read_config_file();

    let mut node = Node::new(node_meta);
    node.start().await;

    let mut peer_stream = initiate_handshake(node.addr()).await.unwrap();

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
