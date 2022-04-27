use crate::{
    setup::node::{Action, Node},
    tools::{synthetic_node::SyntheticNode, TIMEOUT},
    wait_until,
};

#[tokio::test]
async fn when_node_receives_connection() {
    // ZG-CONFORMANCE-001

    // Spin up a node instance.
    let mut node = Node::new().unwrap();
    node.initial_action(Action::WaitForConnection)
        .start()
        .await
        .unwrap();

    // Create a synthetic node and enable handshaking.
    let synthetic_node = SyntheticNode::builder()
        .with_full_handshake()
        .build()
        .await
        .unwrap();

    // Connect to the node and initiate the handshake.
    synthetic_node.connect(node.addr()).await.unwrap();

    // This is only set post-handshake (if enabled).
    assert!(synthetic_node.is_connected(node.addr()));

    // Gracefully shut down the nodes.
    synthetic_node.shut_down().await;
    node.stop().unwrap();
}

#[tokio::test]
async fn when_node_initiates_connection() {
    // ZG-CONFORMANCE-002

    // Create a synthetic node and enable handshaking.
    let synthetic_node = SyntheticNode::builder()
        .with_full_handshake()
        .build()
        .await
        .unwrap();

    // Spin up a node and set the synthetic node as an initial peer.
    let mut node = Node::new().unwrap();
    node.initial_peers(vec![synthetic_node.listening_addr()])
        .start()
        .await
        .unwrap();

    // Check the connection has been established (this is only set post-handshake). We can't check
    // for the addr as nodes use ephemeral addresses when initiating connections.
    wait_until!(TIMEOUT, synthetic_node.num_connected() == 1);

    // Gracefully shut down the nodes.
    synthetic_node.shut_down().await;
    node.stop().unwrap();
}
