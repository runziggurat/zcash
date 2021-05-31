use crate::{
    assert_matches,
    helpers::{autorespond_and_expect_disconnect, initiate_handshake, initiate_version_exchange},
    protocol::{
        message::{constants::MAX_MESSAGE_LEN, Message},
        payload::Version,
    },
    setup::{
        config::new_local_addr,
        node::{Action, Node},
    },
    tests::resistance::{seeded_rng, ITERATIONS},
};

use tokio::{
    io::AsyncWriteExt,
    net::{TcpListener, TcpStream},
};

use rand::Rng;
use rand_chacha::ChaCha8Rng;

#[tokio::test]
async fn zeroes_pre_handshake() {
    // ZG-RESISTANCE-001 (part 1)
    //
    // zebra: sends a version before disconnecting.
    // zcashd: disconnects immediately (log: `INFO main: PROCESSMESSAGE: INVALID MESSAGESTART peer=1`).

    let mut rng = seeded_rng();
    let payloads = zeroes(&mut rng, ITERATIONS);

    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection(new_local_addr()))
        .start()
        .await;

    for payload in payloads {
        let mut peer_stream = TcpStream::connect(node.addr()).await.unwrap();
        let _ = peer_stream.write_all(&payload).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

#[tokio::test]
async fn zeroes_during_handshake_responder_side() {
    // ZG-RESISTANCE-002 (part 1)
    //
    // zebra: responds with verack before disconnecting.
    // zcashd: disconnects immediately.

    let mut rng = seeded_rng();
    let payloads = zeroes(&mut rng, ITERATIONS);

    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection(new_local_addr()))
        .start()
        .await;

    for payload in payloads {
        let mut peer_stream = initiate_version_exchange(node.addr()).await.unwrap();

        // Write zeroes in place of Verack.
        let _ = peer_stream.write_all(&payload).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

#[tokio::test]
async fn zeroes_for_version_when_node_initiates_handshake() {
    // ZG-RESISTANCE-003 (part 1)
    //
    // zebra: disconnects immediately.
    // zcashd: disconnects immediately.
    //
    // Note: zcashd is two orders of magnitude slower (~52 vs ~0.5 seconds)

    let mut rng = seeded_rng();
    let mut payloads = zeroes(&mut rng, ITERATIONS);

    // create tcp listeners for peer set (port is only assigned on tcp bind)
    let mut listeners = Vec::with_capacity(payloads.len());
    for _ in 0..payloads.len() {
        listeners.push(TcpListener::bind(new_local_addr()).await.unwrap());
    }

    // get list of peer addresses to pass to node
    let peer_addresses = listeners
        .iter()
        .map(|listener| listener.local_addr().unwrap())
        .collect::<Vec<_>>();

    // start peer processes
    let mut peer_handles = Vec::with_capacity(listeners.len());
    for peer in listeners {
        let payload = payloads.pop().unwrap();
        peer_handles.push(tokio::time::timeout(
            tokio::time::Duration::from_secs(120),
            tokio::spawn(async move {
                // Await connection and receive version
                let (mut peer_stream, _) = peer.accept().await.unwrap();
                let version = Message::read_from_stream(&mut peer_stream).await.unwrap();
                assert_matches!(version, Message::Version(..));

                // send bad version
                let _ = peer_stream.write_all(&payload).await;

                autorespond_and_expect_disconnect(&mut peer_stream).await;
            }),
        ));
    }

    let mut node: Node = Default::default();
    node.initial_action(Action::None)
        .initial_peers(peer_addresses)
        .start()
        .await;

    // join the peer processes
    for handle in peer_handles {
        handle.await.unwrap().unwrap();
    }

    node.stop().await;
}

#[tokio::test]
async fn zeroes_for_verack_when_node_initiates_handshake() {
    // ZG-RESISTANCE-004 (part 1)
    //
    // zebra: disconnects immediately.
    // zcashd: sends GetAddr, Ping, GetHeaders before disconnecting
    //
    // Note: zcashd is two orders of magnitude slower (~52 vs ~0.5 seconds)

    let mut rng = seeded_rng();
    let mut payloads = zeroes(&mut rng, ITERATIONS);

    // create tcp listeners for peer set (port is only assigned on tcp bind)
    let mut listeners = Vec::with_capacity(payloads.len());
    for _ in 0..payloads.len() {
        listeners.push(TcpListener::bind(new_local_addr()).await.unwrap());
    }

    // get list of peer addresses to pass to node
    let peer_addresses = listeners
        .iter()
        .map(|listener| listener.local_addr().unwrap())
        .collect::<Vec<_>>();

    // start peer processes
    let mut peer_handles = Vec::with_capacity(listeners.len());
    for peer in listeners {
        let payload = payloads.pop().unwrap();
        peer_handles.push(tokio::time::timeout(
            tokio::time::Duration::from_secs(120),
            tokio::spawn(async move {
                // Await connection and receive version
                let (mut peer_stream, _) = peer.accept().await.unwrap();
                let version = Message::read_from_stream(&mut peer_stream).await.unwrap();
                assert_matches!(version, Message::Version(..));

                // send version, receive verack
                Message::Version(Version::new(
                    peer_stream.peer_addr().unwrap(),
                    peer_stream.local_addr().unwrap(),
                ))
                .write_to_stream(&mut peer_stream)
                .await
                .unwrap();
                let verack = Message::read_from_stream(&mut peer_stream).await.unwrap();
                assert_matches!(verack, Message::Verack);

                // send bad verack
                let _ = peer_stream.write_all(&payload).await;
                autorespond_and_expect_disconnect(&mut peer_stream).await;
            }),
        ));
    }

    let mut node: Node = Default::default();
    node.initial_action(Action::None)
        .initial_peers(peer_addresses)
        .start()
        .await;

    // join the peer processes
    for handle in peer_handles {
        handle.await.unwrap().unwrap();
    }

    node.stop().await;
}

#[tokio::test]
async fn zeroes_post_handshake() {
    // ZG-RESISTANCE-005 (part 1)
    //
    // zebra: disconnects.
    // zcashd: responds with ping and getheaders before disconnecting.

    let mut rng = seeded_rng();
    let payloads = zeroes(&mut rng, ITERATIONS);

    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection(new_local_addr()))
        .start()
        .await;

    for payload in payloads {
        let mut peer_stream = initiate_handshake(node.addr()).await.unwrap();

        // Write zeroes in place of Verack.
        let _ = peer_stream.write_all(&payload).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

// Random length zeroes.
fn zeroes(rng: &mut ChaCha8Rng, n: usize) -> Vec<Vec<u8>> {
    (0..n)
        .map(|_| {
            let random_len: usize = rng.gen_range(1..(MAX_MESSAGE_LEN * 2));
            vec![0u8; random_len]
        })
        .collect()
}
