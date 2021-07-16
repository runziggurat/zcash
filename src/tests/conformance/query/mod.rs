use std::{io, time::Duration};

use crate::{
    protocol::{
        message::Message,
        payload::{block::Block, Nonce},
    },
    setup::node::{Action, Node},
    tools::synthetic_node::SyntheticNode,
};

mod get_blocks;
mod get_data;
mod get_header;

lazy_static::lazy_static!(
    /// The blocks that the node is seeded with for this test module.
    static ref SEED_BLOCKS: Vec<Block> = {
        Block::initial_testnet_blocks()
    };
);

/// Starts a node seeded with the initial testnet chain, connects a single
/// SyntheticNode and sends a query. The node's responses to this query is
/// then returned.
async fn run_test_query(query: Message) -> io::Result<Vec<Message>> {
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
