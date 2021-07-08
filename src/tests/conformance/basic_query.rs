use crate::{
    protocol::{
        message::Message,
        payload::{
            block::{Block, LocatorHashes},
            Hash, Inv, Nonce,
        },
    },
    setup::node::{Action, Node},
    tools::{synthetic_node::SyntheticNode, TIMEOUT},
};

use assert_matches::assert_matches;

#[tokio::test]
async fn basic_query_response_seeded() {
    // ZG-CONFORMANCE-010, node is seeded with data
    //
    // The node responds with the correct messages. Message correctness is naively verified through successful encoding/decoding.
    //
    // `Ping` expects `Pong`.
    // `GetAddr` expects `Addr`.
    // `Mempool` expects `Inv`.
    // `Getblocks` expects `Inv`.
    // `GetData(tx_hash)` expects `Tx`.
    // `GetData(block_hash)` expects `Blocks`.
    // `GetHeaders` expects `Headers`.
    //
    // zebra: DoS `GetData` spam due to auto-response
    // zcashd: ignores the following messages
    //             - GetAddr
    //             - MemPool
    //
    //         GetData(tx) returns NotFound (which is correct),
    //         because we currently can't seed a mempool.
    //

    let genesis_block = Block::testnet_genesis();

    // Spin up a node instance.
    let mut node = Node::new().unwrap();
    node.initial_action(Action::SeedWithTestnetBlocks(11))
        .start()
        .await
        .unwrap();

    // Create a synthetic node.
    let mut synthetic_node = SyntheticNode::builder()
        .with_full_handshake()
        .with_all_auto_reply()
        .build()
        .await
        .unwrap();

    // Connect to the node and initiate handshake.
    synthetic_node.connect(node.addr()).await.unwrap();

    // Ping/Pong.
    {
        let ping_nonce = Nonce::default();
        synthetic_node
            .send_direct_message(node.addr(), Message::Ping(ping_nonce))
            .await
            .unwrap();

        // Verify the nonce matches.
        let (_, pong) = synthetic_node.recv_message_timeout(TIMEOUT).await.unwrap();
        assert_matches!(pong, Message::Pong(pong_nonce) if pong_nonce == ping_nonce);
    }

    // GetAddr/Addr.
    {
        synthetic_node
            .send_direct_message(node.addr(), Message::GetAddr)
            .await
            .unwrap();

        let (_, addr) = synthetic_node.recv_message_timeout(TIMEOUT).await.unwrap();
        assert_matches!(addr, Message::Addr(..));
    }

    // MemPool/Inv.
    {
        synthetic_node
            .send_direct_message(node.addr(), Message::MemPool)
            .await
            .unwrap();

        let (_, inv) = synthetic_node.recv_message_timeout(TIMEOUT).await.unwrap();
        assert_matches!(inv, Message::Inv(..));
    }

    // GetBlocks/Inv (requesting testnet genesis).
    {
        synthetic_node
            .send_direct_message(
                node.addr(),
                Message::GetBlocks(LocatorHashes::new(
                    vec![genesis_block.double_sha256().unwrap()],
                    Hash::zeroed(),
                )),
            )
            .await
            .unwrap();

        let (_, inv) = synthetic_node.recv_message_timeout(TIMEOUT).await.unwrap();
        assert_matches!(inv, Message::Inv(..));
    }

    // GetData/Tx.
    {
        synthetic_node
            .send_direct_message(
                node.addr(),
                Message::GetData(Inv::new(vec![genesis_block.txs[0].inv_hash()])),
            )
            .await
            .unwrap();

        let (_, tx) = synthetic_node.recv_message_timeout(TIMEOUT).await.unwrap();
        assert_matches!(tx, Message::Tx(..));
    }

    // GetData/Block.
    {
        synthetic_node
            .send_direct_message(
                node.addr(),
                Message::GetData(Inv::new(vec![Block::testnet_2().inv_hash()])),
            )
            .await
            .unwrap();

        let (_, block) = synthetic_node.recv_message_timeout(TIMEOUT).await.unwrap();
        assert_matches!(block, Message::Block(..));
    }

    // GetHeaders/Headers.
    {
        synthetic_node
            .send_direct_message(
                node.addr(),
                Message::GetHeaders(LocatorHashes::new(
                    vec![genesis_block.double_sha256().unwrap()],
                    Hash::zeroed(),
                )),
            )
            .await
            .unwrap();

        let (_, headers) = synthetic_node.recv_message_timeout(TIMEOUT).await.unwrap();
        assert_matches!(headers, Message::Headers(..));
    }

    // Gracefully shut down the nodes.
    synthetic_node.shut_down();
    node.stop().unwrap();
}

#[tokio::test]
async fn basic_query_response_unseeded() {
    // ZG-CONFORMANCE-010, node is *not* seeded with data
    //
    // The node responds with the correct messages. Message correctness is naively verified through successful encoding/decoding.
    //
    // `GetData(tx_hash)` expects `NotFound`.
    // `GetData(block_hash)` expects `NotFound`.
    //
    // The test currently fails for zcashd and zebra
    //
    // Current behaviour:
    //
    //  zebra: DDoS spam due to auto-response
    //  zcashd: Ignores `GetData(block_hash)`

    // GetData messages...
    let tx_inv = Inv::new(vec![Block::testnet_genesis().txs[0].inv_hash()]);
    let block_inv = Inv::new(vec![Block::testnet_2().inv_hash()]);

    let messages = vec![
        // ...with a tx hash...
        (tx_inv.clone(), Message::GetData(tx_inv)),
        // ...and with a block hash.
        (block_inv.clone(), Message::GetData(block_inv)),
    ];

    // Spin up a node instance.
    let mut node = Node::new().unwrap();
    node.initial_action(Action::WaitForConnection)
        .start()
        .await
        .unwrap();

    // Create a synthetic node with message filtering.
    let mut synthetic_node = SyntheticNode::builder()
        .with_full_handshake()
        .with_all_auto_reply()
        .build()
        .await
        .unwrap();

    // Connect to the node and initiate the handshake.
    synthetic_node.connect(node.addr()).await.unwrap();

    for (expected_inv, message) in messages {
        // Send GetData.
        synthetic_node
            .send_direct_message(node.addr(), message)
            .await
            .unwrap();

        // Assert NotFound is returned.
        // FIXME: assert on hash?
        let (_, reply) = synthetic_node.recv_message_timeout(TIMEOUT).await.unwrap();
        let not_found_inv = assert_matches!(reply, Message::NotFound(inv) => inv);
        assert_eq!(not_found_inv, expected_inv);
    }

    // Gracefully shut down the nodes.
    synthetic_node.shut_down();
    node.stop().unwrap();
}
