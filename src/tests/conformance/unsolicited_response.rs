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
    tests::conformance::simple_handshaken_node,
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
    let (mut node, mut synthetic_node) = simple_handshaken_node().await?;

    synthetic_node
        .send_direct_message(node.addr(), message)
        .await?;

    // A response to ping would indicate the previous message was ignored.
    let nonce = Nonce::default();
    let expected_pong = Message::Pong(nonce);
    synthetic_node
        .send_direct_message(node.addr(), Message::Ping(nonce))
        .await?;

    match synthetic_node
        .recv_message_timeout(Duration::from_secs(1))
        .await
    {
        Ok((_, reply)) if reply == expected_pong => {}
        Ok((_, reply)) => {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Expected {:?}, but got {:?}", expected_pong, reply),
            ));
        }
        Err(_timeout) if !synthetic_node.is_connected(node.addr()) => {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "Connection terminated",
            ));
        }
        Err(_timeout) => {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "Read timed out"));
        }
    }

    // Gracefully shut down the nodes.
    synthetic_node.shut_down();
    node.stop()?;

    Ok(())
}
