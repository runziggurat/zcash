use std::net::SocketAddr;

use assert_matches::assert_matches;

use crate::{
    protocol::{
        message::Message,
        payload::{addr::NetworkAddr, Addr},
    },
    setup::node::{Action, Node},
    tools::{
        message_filter::{Filter, MessageFilter},
        synthetic_node::SyntheticNode,
        LONG_TIMEOUT,
    },
    wait_until,
};

#[tokio::test]
async fn eagerly_crawls_network_for_peers() {
    // ZG-CONFORMANCE-013
    //
    // The node crawls the network for new peers and eagerly connects.
    //
    // Test procedure:
    //
    //  1. Create a set of peer nodes, listening concurrently
    //  2. Connect to node with another main peer node
    //  3. Wait for `GetAddr`
    //  4. Send set of peer listener node addresses
    //  5. Expect the node to connect to each peer in the set
    //
    // zcashd: Has different behaviour depending on connection direction.
    //         If we initiate the main connection it sends Ping, GetHeaders,
    //         but never GetAddr.
    //         If the node initiates then it does send GetAddr, but it never connects
    //         to the peers.
    //
    // zebra:  Fails, unless we keep responding on the main connection.
    //         If we do not keep responding then the peer connections take really long to establish,
    //         failing the test completely.
    //
    //         Nu5: fails, caches the addresses but doesn't open new connections, peer protocol
    //         tbc.
    //
    //         Related issues: https://github.com/ZcashFoundation/zebra/pull/2154
    //                         https://github.com/ZcashFoundation/zebra/issues/2163

    // Spin up a node instance.
    let mut node = Node::new().unwrap();
    node.initial_action(Action::WaitForConnection)
        .start()
        .await
        .unwrap();

    // Create 5 synthetic nodes.
    const N: usize = 5;
    let (synthetic_nodes, addrs) = SyntheticNode::builder()
        .with_full_handshake()
        .with_all_auto_reply()
        .build_n(N)
        .await
        .unwrap();

    let addrs = addrs
        .iter()
        .map(|&addr| NetworkAddr::new(addr))
        .collect::<Vec<_>>();

    // Adjust the config so it lets through GetAddr message and start a "main" synthetic node which
    // will provide the peer list.
    let synthetic_node = SyntheticNode::builder()
        .with_full_handshake()
        .with_message_filter(
            MessageFilter::with_all_auto_reply().with_getaddr_filter(Filter::Disabled),
        )
        .build()
        .await
        .unwrap();

    // Connect and handshake.
    synthetic_node.connect(node.addr()).await.unwrap();

    // Expect GetAddr, this used to be necessary, as of Nu5, it may not be anymore.
    // let (_, getaddr) = synthetic_node.recv_message_timeout(TIMEOUT).await.unwrap();
    // assert_matches!(getaddr, Message::GetAddr);

    // Respond with peer list.
    synthetic_node
        .send_direct_message(node.addr(), Message::Addr(Addr::new(addrs)))
        .unwrap();

    // Expect the synthetic nodes to get a connection request from the node.
    for node in synthetic_nodes {
        wait_until!(LONG_TIMEOUT, node.num_connected() == 1);

        node.shut_down().await;
    }

    // Gracefully shut down the node.
    node.stop().unwrap();
}

#[tokio::test]
async fn correctly_lists_peers() {
    // ZG-CONFORMANCE-014
    //
    // The node responds to a `GetAddr` with a list of peers it’s connected to. This command
    // should only be sent once, and by the node initiating the connection.
    //
    // In addition, this test case exercises the known zebra bug: https://github.com/ZcashFoundation/zebra/pull/2120
    //
    // Test procedure
    //      1. Establish N peer listeners
    //      2. Start node which connects to these N peers
    //      3. Create i..M new connections which,
    //          a) Connect to the node
    //          b) Query GetAddr
    //          c) Receive Addr == N peer addresses
    //
    // This test currently fails for both zcashd and zebra.
    //
    // Current behaviour:
    //
    //  zcashd: Never responds. Logs indicate `Unknown command "getaddr" from peer=1` if we initiate
    //          the connection. If the node initiates the connection then the command is recoginized,
    //          but likely ignored (because only the initiating node is supposed to send it).
    //
    //  zebra:  Never responds: "zebrad::components::inbound: ignoring `Peers` request from remote peer during network setup"
    //
    //          Nu5: never responds, not sure why, adding a timeout after the node start removes the setup error.
    //
    //          Can be coaxed into responding by sending a non-empty Addr in
    //          response to node's GetAddr. This still fails as it includes previous inbound
    //          connections in its address book (as in the bug listed above).

    // Create 5 synthetic nodes.
    const N: usize = 5;
    let node_builder = SyntheticNode::builder()
        .with_full_handshake()
        .with_all_auto_reply();
    let (synthetic_nodes, expected_addrs) = node_builder.build_n(N).await.unwrap();

    // Start node with the synthetic nodes as initial peers.
    let mut node = Node::new().unwrap();
    node.initial_action(Action::WaitForConnection)
        .initial_peers(expected_addrs.clone())
        .start()
        .await
        .unwrap();

    // This fixes the "setup incomplete" issue.
    // tokio::time::sleep(std::time::Duration::from_secs(10)).await;

    // Connect to node and request GetAddr. We perform multiple iterations to exercise the #2120
    // zebra bug.
    for _ in 0..N {
        let mut synthetic_node = node_builder.build().await.unwrap();

        synthetic_node.connect(node.addr()).await.unwrap();
        synthetic_node
            .send_direct_message(node.addr(), Message::GetAddr)
            .unwrap();

        let (_, addr) = synthetic_node
            .recv_message_timeout(LONG_TIMEOUT)
            .await
            .unwrap();
        let addrs = assert_matches!(addr, Message::Addr(addrs) => addrs);

        // Check that ephemeral connections were not gossiped.
        let addrs: Vec<SocketAddr> = addrs.iter().map(|network_addr| network_addr.addr).collect();
        assert_eq!(addrs, expected_addrs);

        synthetic_node.shut_down().await;
    }

    // Gracefully shut down nodes.
    for synthetic_node in synthetic_nodes {
        synthetic_node.shut_down().await;
    }

    node.stop().unwrap();
}
