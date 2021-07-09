//! Contains test cases which cover ZG-CONFORMANCE-009.
//!
//! The node should ignore the following unsolicited messages:
//!
//!  Reject, NotFound, Pong, Tx, Block, Header, Addr

use std::{io, time::Duration};

use crate::{
    protocol::{
        message::Message,
        payload::{
            block::{Block, Headers},
            Addr, Inv, Nonce,
        },
    },
    setup::node::{Action, Node},
    tools::synthetic_node::SyntheticNode,
};

#[tokio::test]
async fn pong() {
    // zcashd: pass
    // zebra:  pass
    run_test_case(Message::Pong(Nonce::default()))
        .await
        .unwrap();
}

#[tokio::test]
async fn headers() {
    // zcashd: pass
    // zebra:  pass
    run_test_case(Message::Headers(Headers::empty()))
        .await
        .unwrap();
}

#[tokio::test]
async fn addr() {
    // zcashd: pass
    // zebra:  pass
    run_test_case(Message::Addr(Addr::empty())).await.unwrap();
}

#[tokio::test]
async fn block() {
    // zcashd: pass
    // zebra:  pass
    run_test_case(Message::Block(Box::new(Block::testnet_genesis())))
        .await
        .unwrap();
}

#[tokio::test]
async fn not_found() {
    // zcashd: pass
    // zebra:  pass
    run_test_case(Message::NotFound(Inv::new(vec![
        Block::testnet_1().txs[0].inv_hash()
    ])))
    .await
    .unwrap();
}

#[tokio::test]
async fn tx() {
    // zcashd: pass
    // zebra:  pass
    run_test_case(Message::Tx(Block::testnet_2().txs[0].clone()))
        .await
        .unwrap();
}

async fn run_test_case(message: Message) -> io::Result<()> {
    // Setup a fully handshaken connection between a node and synthetic node.
    let mut node = Node::new()?;
    node.initial_action(Action::WaitForConnection)
        .start()
        .await?;
    let mut synthetic_node = SyntheticNode::builder()
        .with_full_handshake()
        .with_all_auto_reply()
        .build()
        .await?;
    synthetic_node.connect(node.addr()).await?;

    synthetic_node
        .send_direct_message(node.addr(), message)
        .await?;

    // A response to ping would indicate the previous message was ignored.
    let result = synthetic_node
        .ping_pong_timeout(node.addr(), Duration::from_secs(1))
        .await;

    // Gracefully shut down the nodes.
    synthetic_node.shut_down();
    node.stop()?;

    result?;
    Ok(())
}
