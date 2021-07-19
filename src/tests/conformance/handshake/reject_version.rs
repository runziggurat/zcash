use crate::{
    protocol::{
        message::Message,
        payload::{reject::CCode, Version},
    },
    setup::node::{Action, Node},
    tools::{synthetic_node::SyntheticNode, TIMEOUT},
    wait_until,
};

use assert_matches::assert_matches;

#[tokio::test]
async fn reusing_nonce() {
    // ZG-CONFORMANCE-006
    //
    // The node rejects connections reusing its nonce (usually indicative of self-connection).
    //
    // zebra: closes the write half of the stream, doesn't close the socket.
    // zcashd: closes the write half of the stream, doesn't close the socket.

    // Create a synthetic node, no handshake, no message filters.
    let mut synthetic_node = SyntheticNode::builder().build().await.unwrap();

    // Spin up a node instance with the synthetic node set as an initial peer.
    let mut node = Node::new().unwrap();
    node.initial_peers(vec![synthetic_node.listening_addr()])
        .start()
        .await
        .unwrap();

    // Receive a Version.
    let (source, version) = synthetic_node.recv_message_timeout(TIMEOUT).await.unwrap();
    let nonce = assert_matches!(version, Message::Version(version) => version.nonce);

    // Send a Version.
    let mut bad_version = Version::new(node.addr(), synthetic_node.listening_addr());
    bad_version.nonce = nonce;
    synthetic_node
        .send_direct_message(source, Message::Version(bad_version))
        .await
        .unwrap();

    // Assert on disconnect.
    wait_until!(TIMEOUT, synthetic_node.num_connected() == 0);

    // Gracefully shut down the nodes.
    synthetic_node.shut_down();
    node.stop().unwrap();
}

#[tokio::test]
async fn with_obsolete_version_numbers() {
    // ZG-CONFORMANCE-007
    //
    // The node rejects connections with obsolete node versions.
    //
    // zebra: doesn't send a reject, closes the write half of the stream, doesn't close the socket.
    // zcashd: sends reject before closing the write half of the stream, doesn't close the socket.

    let obsolete_version_numbers: Vec<u32> = (170000..170002).collect();

    // Spin up a node instance.
    let mut node = Node::new().unwrap();
    node.initial_action(Action::WaitForConnection)
        .start()
        .await
        .unwrap();

    // Configuration for all synthetic nodes, no handshake, no message filter.
    let node_builder = SyntheticNode::builder();

    for obsolete_version_number in obsolete_version_numbers {
        // Create a synthetic node.
        let mut synthetic_node = node_builder.build().await.unwrap();

        // Connect to the node and send a Version with an obsolete version.
        synthetic_node.connect(node.addr()).await.unwrap();
        synthetic_node
            .send_direct_message(
                node.addr(),
                Message::Version(
                    Version::new(node.addr(), synthetic_node.listening_addr())
                        .with_version(obsolete_version_number),
                ),
            )
            .await
            .unwrap();

        // Expect a reject message.
        let (_, reject) = synthetic_node.recv_message_timeout(TIMEOUT).await.unwrap();
        assert_matches!(reject, Message::Reject(reject) if reject.ccode == CCode::Obsolete);

        // Expect the connection to be dropped.
        wait_until!(TIMEOUT, synthetic_node.num_connected() == 0);

        // Gracefull shut down the synthetic node.
        synthetic_node.shut_down();
    }

    // Gracefully shut down the node.
    node.stop().unwrap();
}
