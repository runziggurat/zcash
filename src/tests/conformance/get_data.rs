//! Contains test cases which cover ZG-CONFORMANCE-017
//!
//! The node responds to `GetData` requests with the appropriate transaction or block as requested by the peer.
//!
//! Note: Zebra does not support seeding with chain data and as such cannot run any of these tests successfully.
//!
//! Note: ZCashd currently ignores requests for non-existant blocks. We expect a [`Message::NotFound`] response.

use std::{io, time::Duration};

use crate::{
    protocol::{
        message::Message,
        payload::{
            block::Block,
            inv::{InvHash, ObjectKind},
            Hash, Inv, Nonce,
        },
    },
    setup::node::{Action, Node},
    tools::synthetic_node::SyntheticNode,
};

lazy_static::lazy_static!(
    /// The blocks that the node is seeded with for this test module.
    static ref SEED_BLOCKS: Vec<Block> = {
        Block::initial_testnet_blocks()
    };

    /// InvHashes of the blocks that the node is seeded with.
    static ref SEED_BLOCK_HASHES: Vec<InvHash> = {
        SEED_BLOCKS.iter().map(|block| block.inv_hash()).collect()
    };
);

mod single_block {
    use super::*;

    #[tokio::test]
    async fn block_last() {
        // zcashd: pass
        let block = SEED_BLOCKS.last().unwrap().clone();
        let query = Message::GetData(Inv::new(vec![block.inv_hash()]));
        let expected = vec![Message::Block(block.into())];
        let response = run_test_case(query).await.unwrap();
        assert_eq!(response, expected);
    }

    #[tokio::test]
    async fn block_first() {
        // zcashd: pass
        let block = SEED_BLOCKS.first().unwrap().clone();
        let query = Message::GetData(Inv::new(vec![block.inv_hash()]));
        let expected = vec![Message::Block(block.into())];
        let response = run_test_case(query).await.unwrap();
        assert_eq!(response, expected);
    }

    #[tokio::test]
    async fn block_5() {
        // zcashd: pass
        let block = SEED_BLOCKS[5].clone();
        let query = Message::GetData(Inv::new(vec![block.inv_hash()]));
        let expected = vec![Message::Block(block.into())];
        let response = run_test_case(query).await.unwrap();
        assert_eq!(response, expected);
    }

    #[tokio::test]
    async fn non_existant() {
        // zcashd: fail (ignores non-existant block)
        let inv = Inv::new(vec![InvHash::new(ObjectKind::Block, Hash::new([17; 32]))]);
        let query = Message::GetData(inv.clone());
        let expected = vec![Message::NotFound(inv)];
        let response = run_test_case(query).await.unwrap();
        assert_eq!(response, expected);
    }
}

mod multiple_blocks {
    use super::*;

    #[tokio::test]
    async fn all() {
        // zcashd: pass
        let blocks = &SEED_BLOCKS;
        let inv_hash = blocks.iter().map(|block| block.inv_hash()).collect();
        let query = Message::GetData(Inv::new(inv_hash));
        let expected = blocks
            .iter()
            .map(|block| Message::Block(Box::new(block.clone())))
            .collect::<Vec<_>>();
        let response = run_test_case(query).await.unwrap();
        assert_eq!(response, expected);
    }

    #[tokio::test]
    async fn out_of_order() {
        // zcashd: pass
        let blocks = vec![&SEED_BLOCKS[3], &SEED_BLOCKS[1], &SEED_BLOCKS[7]];
        let inv_hash = blocks.iter().map(|block| block.inv_hash()).collect();
        let query = Message::GetData(Inv::new(inv_hash));
        let expected = blocks
            .iter()
            .map(|&block| Message::Block(Box::new(block.clone())))
            .collect::<Vec<_>>();
        let response = run_test_case(query).await.unwrap();
        assert_eq!(response, expected);
    }

    #[tokio::test]
    async fn blocks_1_to_8() {
        // zcashd: pass
        let blocks = &SEED_BLOCKS[1..=8];
        let inv_hash = blocks.iter().map(|block| block.inv_hash()).collect();
        let query = Message::GetData(Inv::new(inv_hash));
        let expected = blocks
            .iter()
            .map(|block| Message::Block(Box::new(block.clone())))
            .collect::<Vec<_>>();
        let response = run_test_case(query).await.unwrap();
        assert_eq!(response, expected);
    }

    #[tokio::test]
    async fn non_existant_blocks() {
        // zcashd: fails (ignores non-existant blocks).
        let inv = Inv::new(vec![
            InvHash::new(ObjectKind::Block, Hash::new([17; 32])),
            InvHash::new(ObjectKind::Block, Hash::new([211; 32])),
            InvHash::new(ObjectKind::Block, Hash::new([74; 32])),
        ]);

        let query = Message::GetData(inv.clone());
        let expected = vec![Message::NotFound(inv)];
        let response = run_test_case(query).await.unwrap();
        assert_eq!(response, expected);
    }

    #[tokio::test]
    async fn mixed_existant_and_non_existant_blocks() {
        // Test a mixture of existant and non-existant blocks
        // interwoven together.
        //
        // We expect the response to contain a Block for each
        // existing request, and a single NotFound containing
        // hashes for all non-existant blocks. The order of
        // these is undefined, but since zcashd currently
        // does not send NotFound at all, it does not matter.
        //
        // zcashd: fails (ignores non-existant blocks).

        let non_existant_inv = vec![
            InvHash::new(ObjectKind::Block, Hash::new([17; 32])),
            InvHash::new(ObjectKind::Block, Hash::new([211; 32])),
            InvHash::new(ObjectKind::Block, Hash::new([74; 32])),
        ];
        let blocks = SEED_BLOCKS
            .iter()
            .skip(3)
            .take(non_existant_inv.len())
            .collect::<Vec<_>>();

        let expected_blocks = blocks
            .iter()
            .map(|&block| Message::Block(Box::new(block.clone())))
            .collect::<Vec<_>>();
        let expected_non_existant = Message::NotFound(Inv::new(non_existant_inv.clone()));

        let mixed_inv = blocks
            .iter()
            .map(|block| block.inv_hash())
            .zip(non_existant_inv)
            .map(|(a, b)| [a, b])
            .into_iter()
            .flatten()
            .collect();

        let query = Message::GetData(Inv::new(mixed_inv));
        let response = run_test_case(query).await.unwrap();

        // Should contain expected_blocks[..] and expected_non_existant. Not sure
        // what order we should expect (if any). But since the node currently ignores
        // non-existant block queries we are free to assume any order.
        let mut expected = expected_blocks;
        expected.push(expected_non_existant);
        assert_eq!(response, expected);
    }
}

/// Starts a node seeded with the initial testnet chain, connects a single
/// SyntheticNode and sends a query. The node's responses to this query is
/// then returned.
async fn run_test_case(query: Message) -> io::Result<Vec<Message>> {
    // Spin up a node instance with knowledge of the initial testnet-chain.
    let mut node = Node::new().unwrap();
    node.initial_action(Action::SeedWithTestnetBlocks(SEED_BLOCKS.len()))
        .start()
        .await?;

    // Create a synthetic node.
    let mut synthetic_node = SyntheticNode::builder()
        .with_full_handshake()
        .with_all_auto_reply()
        .build()
        .await?;

    // Connect to the node and initiate handshake.
    synthetic_node.connect(node.addr()).await?;

    // Send the query.
    synthetic_node
        .send_direct_message(node.addr(), query)
        .await?;

    // Send a Ping - once we receive the matching Pong we know our query has been fully processed.
    let nonce = Nonce::default();
    synthetic_node
        .send_direct_message(node.addr(), Message::Ping(nonce))
        .await?;

    // Receive messages until we receive the matching Pong, or we timeout.
    const RECV_TIMEOUT: Duration = Duration::from_millis(100);
    let mut messages = Vec::new();
    loop {
        match synthetic_node.recv_message_timeout(RECV_TIMEOUT).await? {
            (_, Message::Pong(rx_nonce)) if rx_nonce == nonce => break,
            (_, message) => messages.push(message),
        }
    }

    // Gracefully shut down the nodes.
    synthetic_node.shut_down();
    node.stop()?;

    Ok(messages)
}
