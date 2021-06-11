use crate::{
    helpers::{autorespond_and_expect_disconnect, initiate_handshake, initiate_version_exchange},
    protocol::{message::*, payload::Version},
    setup::{
        config::new_local_addr,
        node::{Action, Node},
    },
    tests::resistance::{seeded_rng, COMMANDS_WITH_PAYLOADS, ITERATIONS},
};

use assert_matches::assert_matches;
use rand::{distributions::Standard, prelude::SliceRandom, Rng};
use rand_chacha::ChaCha8Rng;
use tokio::{
    io::AsyncWriteExt,
    net::{TcpListener, TcpStream},
};

#[tokio::test]
async fn random_bytes_pre_handshake() {
    // ZG-RESISTANCE-001 (part 2)
    //
    // zebra: sends a version before disconnecting.
    // zcashd: ignores the bytes and disconnects.

    let mut rng = seeded_rng();
    let payloads = random_bytes(&mut rng, ITERATIONS);

    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection).start().await;

    for payload in payloads {
        let mut peer_stream = TcpStream::connect(node.addr()).await.unwrap();
        let _ = peer_stream.write_all(&payload).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

#[tokio::test]
async fn random_bytes_during_handshake_responder_side() {
    // ZG-RESISTANCE-002 (part 2)
    //
    // zebra: responds with verack before disconnecting.
    // zcashd: responds with verack, pong and getheaders before disconnecting.

    let mut rng = seeded_rng();
    let payloads = random_bytes(&mut rng, ITERATIONS);

    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection).start().await;

    for payload in payloads {
        let mut peer_stream = initiate_version_exchange(node.addr()).await.unwrap();

        // Write random bytes in place of Verack.
        let _ = peer_stream.write_all(&payload).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

#[tokio::test]
async fn random_bytes_for_version_when_node_initiates_handshake() {
    // ZG-RESISTANCE-003 (part 2)
    //
    // zebra: disconnects immediately.
    // zcashd: disconnects immediately.
    //
    // Note: zcashd is two orders of magnitude slower (~52 vs ~0.5 seconds)

    let mut rng = seeded_rng();
    let mut payloads = random_bytes(&mut rng, ITERATIONS);

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
async fn random_bytes_for_verack_when_node_initiates_handshake() {
    // ZG-RESISTANCE-004 (part 2)
    //
    // zebra: disconnects immediately.
    // zcashd: sometimes (~10%) sends GetAddr before disconnecting
    //
    // Note: zcashd is two orders of magnitude slower (~52 vs ~0.5 seconds)

    let mut rng = seeded_rng();
    let mut payloads = random_bytes(&mut rng, ITERATIONS);

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
async fn random_bytes_post_handshake() {
    // ZG-RESISTANCE-005 (part 2)
    //
    // zebra: disconnects.
    // zcashd: sends ping, getheaders and disconnects.

    let mut rng = seeded_rng();
    let payloads = random_bytes(&mut rng, ITERATIONS);

    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection).start().await;

    for payload in payloads {
        let mut peer_stream = initiate_handshake(node.addr()).await.unwrap();

        // Write random bytes in place of Verack.
        let _ = peer_stream.write_all(&payload).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

#[tokio::test]
async fn metadata_compliant_random_bytes_pre_handshake() {
    // ZG-RESISTANCE-001 (part 3)
    //
    // zebra: breaks with a version command in header, otherwise sends verack before closing the
    // connection.
    // zcashd: just ignores the message and doesn't disconnect.

    // Payloadless messages are omitted.
    let mut rng = seeded_rng();
    let payloads = metadata_compliant_random_bytes(&mut rng, ITERATIONS, &COMMANDS_WITH_PAYLOADS);

    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection).start().await;

    for (header, payload) in payloads {
        let mut peer_stream = TcpStream::connect(node.addr()).await.unwrap();
        let _ = header.write_to_stream(&mut peer_stream).await;
        let _ = peer_stream.write_all(&payload).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

#[tokio::test]
async fn metadata_compliant_random_bytes_during_handshake_responder_side() {
    // ZG-RESISTANCE-002 (part 3)
    //
    // zebra: breaks with a version command in header, otherwise sends verack before closing the
    // connection.
    // zcashd: responds with reject, ccode malformed and doesn't disconnect.

    // Payloadless messages are omitted.
    let mut rng = seeded_rng();
    let payloads = metadata_compliant_random_bytes(&mut rng, ITERATIONS, &COMMANDS_WITH_PAYLOADS);

    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection).start().await;

    for (header, payload) in payloads {
        let mut peer_stream = initiate_version_exchange(node.addr()).await.unwrap();

        // Write random bytes in place of Verack.
        let _ = header.write_to_stream(&mut peer_stream).await;
        let _ = peer_stream.write_all(&payload).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

#[tokio::test]
async fn metadata_compliant_random_bytes_for_version_when_node_initiates_handshake() {
    // ZG-RESISTANCE-003 (part 3)
    //
    // zebra: breaks with a version command in header, otherwise sends verack before closing the
    //        connection.
    // zcashd: just ignores the message and doesn't disconnect.
    //
    // Note: zcashd is two orders of magnitude slower (~52 vs ~0.5 seconds)

    // Payloadless messages are omitted.
    let mut rng = seeded_rng();
    let mut payloads =
        metadata_compliant_random_bytes(&mut rng, ITERATIONS, &COMMANDS_WITH_PAYLOADS);

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
        let (header, payload) = payloads.pop().unwrap();
        peer_handles.push(tokio::time::timeout(
            tokio::time::Duration::from_secs(120),
            tokio::spawn(async move {
                // Await connection and receive version
                let (mut peer_stream, _) = peer.accept().await.unwrap();
                let version = Message::read_from_stream(&mut peer_stream).await.unwrap();
                assert_matches!(version, Message::Version(..));

                // send bad version
                let _ = header.write_to_stream(&mut peer_stream).await;
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
async fn metadata_compliant_random_bytes_for_verack_when_node_initiates_handshake() {
    // ZG-RESISTANCE-004 (part 3)
    //
    // zebra: breaks with a version command in header, otherwise sends verack before closing the
    //        connection.
    // zcashd: Sends GetAddr, Ping, GetHeaders
    //         Sometimes responds to malformed Ping's
    //         Never disconnects
    //
    // Caution: zcashd takes extremely long in this test

    // Payloadless messages are omitted.
    let mut rng = seeded_rng();
    let mut payloads =
        metadata_compliant_random_bytes(&mut rng, ITERATIONS, &COMMANDS_WITH_PAYLOADS);

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
        let (header, payload) = payloads.pop().unwrap();
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
                let _ = header.write_to_stream(&mut peer_stream).await;
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
async fn metadata_compliant_random_bytes_post_handshake() {
    // ZG-RESISTANCE-005 (part 3)
    //
    // zebra: breaks with a version command in header, spams getdata, doesn't disconnect.
    // zcashd: does a combination of ignoring messages, returning cc malformed or accepting messages (`addr`)
    // for instance.

    // Payloadless messages are omitted.
    let mut rng = seeded_rng();
    let payloads = metadata_compliant_random_bytes(&mut rng, ITERATIONS, &COMMANDS_WITH_PAYLOADS);

    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection).start().await;

    for (header, payload) in payloads {
        let mut peer_stream = initiate_handshake(node.addr()).await.unwrap();

        // Write random bytes in place of Verack.
        let _ = header.write_to_stream(&mut peer_stream).await;
        let _ = peer_stream.write_all(&payload).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

// Random length, random bytes.
pub fn random_bytes(rng: &mut ChaCha8Rng, n: usize) -> Vec<Vec<u8>> {
    (0..n)
        .map(|_| {
            let random_len: usize = rng.gen_range(1..(64 * 1024));
            let random_payload: Vec<u8> = rng.sample_iter(Standard).take(random_len).collect();

            random_payload
        })
        .collect()
}

// Valid message header, random bytes as message.
pub fn metadata_compliant_random_bytes(
    rng: &mut ChaCha8Rng,
    n: usize,
    commands: &[[u8; 12]],
) -> Vec<(MessageHeader, Vec<u8>)> {
    (0..n)
        .map(|_| {
            let random_len: usize = rng.gen_range(1..(64 * 1024));
            let random_payload: Vec<u8> = rng.sample_iter(Standard).take(random_len).collect();

            let command = commands.choose(rng).unwrap();
            let header = MessageHeader::new(*command, &random_payload);

            (header, random_payload)
        })
        .collect()
}
