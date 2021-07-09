use crate::{
    protocol::{
        message::Message,
        payload::{block::Block, Inv, Nonce},
    },
    setup::node::{Action, Node},
    tools::{
        message_filter::{Filter, MessageFilter},
        synthetic_node::SyntheticNode,
        TIMEOUT,
    },
    wait_until,
};

#[tokio::test]
async fn disconnects_for_trivial_issues() {
    // ZG-CONFORMANCE-011
    //
    // The node disconnects for trivial (non-fuzz, non-malicious) cases.
    //
    // - `Ping` timeout (not tested due to 20minute zcashd timeout).
    // - `Pong` with wrong nonce.
    // - `GetData` with mixed types in inventory list.
    // - `Inv` with mixed types in inventory list.
    // - `Addr` with `NetworkAddr` with no timestamp.
    //
    // Note: Ping with timeout test case is not exercised as the zcashd timeout is
    //       set to 20 minutes, which is simply too long.
    //
    // Note: Addr test requires commenting out the relevant code in the encode
    //       function of NetworkAddr as we cannot encode without a timestamp.
    //
    // This test currently fails for zcashd and zebra.
    //
    // Current behaviour:
    //
    //  zcashd:
    //      GetData(mixed)  - responds to both
    //      Inv(mixed)      - ignores the message
    //      Addr            - Reject(Malformed), but no DC
    //
    //  zebra:
    //      Pong            - ignores the message

    // Spin up a node instance.
    let mut node = Node::new().unwrap();
    node.initial_action(Action::WaitForConnection)
        .start()
        .await
        .unwrap();

    // Configuration letting through ping messages for the first case.
    let node_builder = SyntheticNode::builder()
        .with_full_handshake()
        .with_message_filter(
            MessageFilter::with_all_auto_reply().with_ping_filter(Filter::Disabled),
        );

    // Pong with bad nonce.
    {
        let mut synthetic_node = node_builder.build().await.unwrap();
        synthetic_node.connect(node.addr()).await.unwrap();

        match synthetic_node.recv_message_timeout(TIMEOUT).await.unwrap() {
            (_, Message::Ping(_)) => synthetic_node
                .send_direct_message(node.addr(), Message::Pong(Nonce::default()))
                .await
                .unwrap(),

            message => panic!("Unexpected message while waiting for Ping: {:?}", message),
        }

        wait_until!(TIMEOUT, synthetic_node.num_connected() == 0);
        synthetic_node.shut_down();
    }

    // Update the filter to include ping messages.
    let node_builder = node_builder.with_all_auto_reply();

    // GetData with mixed inventory.
    {
        let synthetic_node = node_builder.build().await.unwrap();
        synthetic_node.connect(node.addr()).await.unwrap();

        let genesis_block = Block::testnet_genesis();
        let mixed_inv = vec![genesis_block.inv_hash(), genesis_block.txs[0].inv_hash()];

        synthetic_node
            .send_direct_message(node.addr(), Message::GetData(Inv::new(mixed_inv.clone())))
            .await
            .unwrap();

        wait_until!(TIMEOUT, synthetic_node.num_connected() == 0);
        synthetic_node.shut_down();
    }

    // Inv with mixed inventory (using non-genesis block since all node's "should" have genesis already,
    // which makes advertising it non-sensical).
    {
        let synthetic_node = node_builder.build().await.unwrap();
        synthetic_node.connect(node.addr()).await.unwrap();

        let block_1 = Block::testnet_1();
        let mixed_inv = vec![block_1.inv_hash(), block_1.txs[0].inv_hash()];

        synthetic_node
            .send_direct_message(node.addr(), Message::Inv(Inv::new(mixed_inv)))
            .await
            .unwrap();

        wait_until!(TIMEOUT, synthetic_node.num_connected() == 0);
        synthetic_node.shut_down();
    }

    // Gracefully shut down the node.
    node.stop().unwrap();
}
