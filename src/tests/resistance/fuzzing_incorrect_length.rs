use crate::{
    helpers::{autorespond_and_expect_disconnect, initiate_handshake, initiate_version_exchange},
    protocol::{
        message::Message,
        payload::{block::Headers, codec::Codec, Addr, Nonce, Version},
    },
    setup::{
        config::new_local_addr,
        node::{Action, Node},
    },
    tests::resistance::{default_fuzz_messages, random_non_valid_u32, seeded_rng, ITERATIONS},
};

use std::sync::Arc;

use assert_matches::assert_matches;
use parking_lot::RwLock;
use rand::prelude::SliceRandom;
use rand_chacha::ChaCha8Rng;
use tokio::{
    io::AsyncWriteExt,
    net::{TcpListener, TcpStream},
};

#[tokio::test]
async fn version_with_incorrect_length_pre_handshake() {
    // ZG-RESISTANCE-001 (part 6)
    //
    // zebra: sends version before disconnecting.
    // zcashd: disconnects.

    let mut rng = seeded_rng();

    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection(new_local_addr()))
        .start()
        .await;

    for _ in 0..ITERATIONS {
        let mut peer_stream = TcpStream::connect(node.addr()).await.unwrap();

        let version =
            Message::Version(Version::new(node.addr(), peer_stream.local_addr().unwrap()));
        let mut message_buffer = vec![];
        let mut header = version.encode(&mut message_buffer).unwrap();

        // Set the length to a random value which isn't the current value.
        header.body_length = random_non_valid_u32(&mut rng, header.body_length);

        let _ = header.write_to_stream(&mut peer_stream).await;
        let _ = peer_stream.write_all(&message_buffer).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

#[tokio::test]
async fn incorrect_length_pre_handshake() {
    // ZG-RESISTANCE-001 (part 6)
    //
    // zebra: disconnects.
    // zcashd: disconnects.

    let mut rng = seeded_rng();

    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection(new_local_addr()))
        .start()
        .await;

    let test_messages = vec![
        Message::GetAddr,
        Message::MemPool,
        Message::Verack,
        Message::Ping(Nonce::default()),
        Message::Pong(Nonce::default()),
        Message::GetAddr,
        Message::Addr(Addr::empty()),
        Message::Headers(Headers::empty()),
        // Message::GetHeaders(LocatorHashes)),
        // Message::GetBlocks(LocatorHashes)),
        // Message::GetData(Inv));
        // Message::Inv(Inv));
        // Message::NotFound(Inv));
    ];

    for _ in 0..ITERATIONS {
        let message = test_messages.choose(&mut rng).unwrap();
        let mut message_buffer = vec![];
        let mut header = message.encode(&mut message_buffer).unwrap();

        // Set the length to a random value which isn't the current value.
        header.body_length = random_non_valid_u32(&mut rng, header.body_length);

        let mut peer_stream = TcpStream::connect(node.addr()).await.unwrap();
        let _ = header.write_to_stream(&mut peer_stream).await;
        let _ = peer_stream.write_all(&message_buffer).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

#[tokio::test]
async fn version_with_incorrect_length_during_handshake_responder_side() {
    // ZG-RESISTANCE-002 (part 6)
    //
    // zebra: sends verack before disconnecting.
    // zcashd: disconnects (after sending verack, ping, getheaders).

    let mut rng = seeded_rng();

    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection(new_local_addr()))
        .start()
        .await;

    for _ in 0..ITERATIONS {
        let mut peer_stream = initiate_version_exchange(node.addr()).await.unwrap();

        let version =
            Message::Version(Version::new(node.addr(), peer_stream.local_addr().unwrap()));
        let mut message_buffer = vec![];
        let mut header = version.encode(&mut message_buffer).unwrap();

        // Set the length to a random value which isn't the current value.
        header.body_length = random_non_valid_u32(&mut rng, header.body_length);

        let _ = header.write_to_stream(&mut peer_stream).await;
        let _ = peer_stream.write_all(&message_buffer).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

#[tokio::test]
async fn version_with_incorrect_length_when_node_initiates_handshake() {
    // ZG-RESISTANCE-003 (part 6, version only)
    //
    // This particular case is considered alone because it is at particular risk of causing
    // troublesome behaviour, as seen with the valid metadata fuzzing against zebra.
    //
    // zebra: disconnects immediately.
    // zcashd: Some messages get ignored and timeout. Most cause an immedietely due to
    //          - main: PROCESSMESSAGE: INVALID MESSAGESTART, or
    //          - net: Oversized message from peer

    let locked_rng = Arc::new(RwLock::new(seeded_rng()));

    // create tcp listeners for peer set (port is only assigned on tcp bind)
    let mut listeners = Vec::with_capacity(ITERATIONS);
    for _ in 0..ITERATIONS {
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
        let peer_rng = locked_rng.clone();
        peer_handles.push(tokio::time::timeout(
            tokio::time::Duration::from_secs(120),
            tokio::spawn(async move {
                // Await connection and receive version
                let (mut peer_stream, _) = peer.accept().await.unwrap();
                let version = Message::read_from_stream(&mut peer_stream).await.unwrap();
                assert_matches!(version, Message::Version(..));

                // send bad version
                let (header, message_buffer) = {
                    let mut rng = peer_rng.write();
                    let version = Message::Version(Version::new(
                        peer_stream.peer_addr().unwrap(),
                        peer_stream.local_addr().unwrap(),
                    ));
                    let mut message_buffer = vec![];
                    let mut header = version.encode(&mut message_buffer).unwrap();

                    // Set the length to a random value which isn't the current value.
                    header.body_length = random_non_valid_u32(&mut rng, header.body_length);

                    (header, message_buffer)
                };

                let _ = header.write_to_stream(&mut peer_stream).await;
                let _ = peer_stream.write_all(&message_buffer).await;

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
async fn version_with_incorrect_length_inplace_of_verack_when_node_initiates_handshake() {
    // ZG-RESISTANCE-004 (part 6, version only)
    //
    // This particular case is considered alone because it is at particular risk of causing
    // troublesome behaviour, as seen with the valid metadata fuzzing against zebra.
    //
    // zebra: disconnects immediately.
    // zcashd: Sends GetAddr, Ping and GetHeaders. Appears to ignore bad verack message.

    let locked_rng = Arc::new(RwLock::new(seeded_rng()));

    // create tcp listeners for peer set (port is only assigned on tcp bind)
    let mut listeners = Vec::with_capacity(ITERATIONS);
    for _ in 0..ITERATIONS {
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
        let peer_rng = locked_rng.clone();
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

                // send bad version instead of verack
                let (header, message_buffer) = {
                    let mut rng = peer_rng.write();
                    let version = Message::Version(Version::new(
                        peer_stream.peer_addr().unwrap(),
                        peer_stream.local_addr().unwrap(),
                    ));
                    let mut message_buffer = vec![];
                    let mut header = version.encode(&mut message_buffer).unwrap();

                    // Set the length to a random value which isn't the current value.
                    header.body_length = random_non_valid_u32(&mut rng, header.body_length);

                    (header, message_buffer)
                };

                let _ = header.write_to_stream(&mut peer_stream).await;
                let _ = peer_stream.write_all(&message_buffer).await;

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
async fn version_with_incorrect_length_post_handshake() {
    // ZG-RESISTANCE-005 (part 6, version only)
    //
    // zebra: disconnects.
    // zcashd: disconnects (after sending ping and getheaders), sometimes hangs.

    let mut rng = seeded_rng();
    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection(new_local_addr()))
        .start()
        .await;

    for _ in 0..ITERATIONS {
        let mut peer_stream = initiate_handshake(node.addr()).await.unwrap();

        let version =
            Message::Version(Version::new(node.addr(), peer_stream.local_addr().unwrap()));
        let mut message_buffer = vec![];
        let mut header = version.encode(&mut message_buffer).unwrap();

        // Set the length to a random value which isn't the current value.
        header.body_length = random_non_valid_u32(&mut rng, header.body_length);

        let _ = header.write_to_stream(&mut peer_stream).await;
        let _ = peer_stream.write_all(&message_buffer).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

#[tokio::test]
async fn incorrect_length_during_handshake_responder_side() {
    // ZG-RESISTANCE-002 (part 6)
    //
    // zebra: disconnects.
    // zcashd: disconnects (after sending verack, ping, getheaders).

    let mut rng = seeded_rng();

    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection(new_local_addr()))
        .start()
        .await;

    let test_messages = default_fuzz_messages();

    for _ in 0..ITERATIONS {
        let message = test_messages.choose(&mut rng).unwrap();
        let mut message_buffer = vec![];
        let mut header = message.encode(&mut message_buffer).unwrap();

        // Set the length to a random value which isn't the current value.
        header.body_length = random_non_valid_u32(&mut rng, header.body_length);

        let mut peer_stream = initiate_version_exchange(node.addr()).await.unwrap();

        // Send message with wrong length in place of valid Verack.
        let _ = header.write_to_stream(&mut peer_stream).await;
        let _ = peer_stream.write_all(&message_buffer).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

#[tokio::test]
async fn incorrect_body_length_inplace_of_version_when_node_initiates_handshake() {
    // ZG-RESISTANCE-003 (part 6)
    //
    // zebra: disconnects
    // zcashd: disconnects

    let mut rng = seeded_rng();

    let test_messages = default_fuzz_messages();

    let mut payloads =
        encode_messages_and_corrupt_body_length_field(&mut rng, ITERATIONS, &test_messages);

    // create tcp listeners for peer set (port is only assigned on tcp bind)
    let mut listeners = Vec::with_capacity(ITERATIONS);
    for _ in 0..ITERATIONS {
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
async fn incorrect_body_length_inplace_of_verack_when_node_initiates_handshake() {
    // ZG-RESISTANCE-004 (part 6)
    //
    // zebra: disconnects, logs:
    //  - an initial peer connection failed e=Serialization(Parse("body length exceeded maximum size"))
    //  - an initial peer connection failed e=Serialization(Parse("supplied magic did not meet expectations")) [?]
    //
    // zcashd: sends GetAddr, Ping, GetHeaders then disconnects

    let mut rng = seeded_rng();

    let test_messages = default_fuzz_messages();

    let mut payloads =
        encode_messages_and_corrupt_body_length_field(&mut rng, ITERATIONS, &test_messages);

    // create tcp listeners for peer set (port is only assigned on tcp bind)
    let mut listeners = Vec::with_capacity(ITERATIONS);
    for _ in 0..ITERATIONS {
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
async fn incorrect_length_post_handshake() {
    // ZG-RESISTANCE-005 (part 6)
    //
    // zebra: disconnects.
    // zcashd: disconnects (after ping and getheaders) but sometimes hangs.

    let mut rng = seeded_rng();
    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection(new_local_addr()))
        .start()
        .await;

    let test_messages = default_fuzz_messages();

    for _ in 0..ITERATIONS {
        let message = test_messages.choose(&mut rng).unwrap();
        let mut message_buffer = vec![];
        let mut header = message.encode(&mut message_buffer).unwrap();

        // Set the length to a random value which isn't the current value.
        header.body_length = random_non_valid_u32(&mut rng, header.body_length);

        let mut peer_stream = initiate_handshake(node.addr()).await.unwrap();

        // Send message with wrong lenght in place of valid Verack.
        let _ = header.write_to_stream(&mut peer_stream).await;
        let _ = peer_stream.write_all(&message_buffer).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

/// Picks `n` random messages from `message_pool`, encodes them and corrupts the body-length field
fn encode_messages_and_corrupt_body_length_field(
    rng: &mut ChaCha8Rng,
    n: usize,
    message_pool: &[Message],
) -> Vec<Vec<u8>> {
    (0..n)
        .map(|_| {
            let message = message_pool.choose(rng).unwrap();

            let mut buffer = vec![];
            let mut header = message.encode(&mut buffer).unwrap();
            header.body_length = random_non_valid_u32(rng, header.body_length);
            header.encode(&mut buffer).unwrap();

            buffer
        })
        .collect()
}
