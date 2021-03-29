use crate::{
    protocol::{message::Message, payload::Version},
    setup::{config::read_config_file, node::Node},
};

use tokio::net::{TcpListener, TcpStream};

#[tokio::test]
async fn handshake_responder_side() {
    // 1. Configure and run node.
    // 2. Send a Version message to the node.
    // 3. Expect a Version back and send Verack.
    // 4. Expect Verack back.

    let (_zig, node_meta) = read_config_file();

    let mut node = Node::new(node_meta);
    node.start().await;

    let mut peer_stream = TcpStream::connect(node.addr()).await.unwrap();

    Message::Version(Version::new(node.addr(), peer_stream.local_addr().unwrap()))
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
async fn handshake_initiator_side() {
    let (zig, node_meta) = read_config_file();

    let listener = TcpListener::bind(zig.new_local_addr()).await.unwrap();

    let mut node = Node::new(node_meta);
    node.initial_peers(vec![listener.local_addr().unwrap().port()])
        .start()
        .await;

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
