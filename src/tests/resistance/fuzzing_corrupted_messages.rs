use crate::{
    helpers::{autorespond_and_expect_disconnect, initiate_handshake, initiate_version_exchange},
    protocol::payload::{codec::Codec, Version},
    setup::{
        config::new_local_addr,
        node::{Action, Node},
    },
    tests::resistance::{default_fuzz_messages, seeded_rng, Message, ITERATIONS},
};

use std::sync::Arc;

use assert_matches::assert_matches;
use parking_lot::RwLock;
use rand::prelude::{Rng, SliceRandom};
use rand_chacha::ChaCha8Rng;
use tokio::{
    io::AsyncWriteExt,
    net::{TcpListener, TcpStream},
};

const CORRUPTION_PROBABILITY: f64 = 0.5;

#[tokio::test]
async fn corrupted_version_pre_handshake() {
    // ZG-RESISTANCE-001 (part 4)
    //
    // This particular case is considered alone because it is at particular risk of causing
    // troublesome behaviour, as seen with the valid metadata fuzzing against zebra.
    //
    // zebra: sends a version before disconnecting.
    // zcashd: ignores message but doesn't disconnect.
    // (log ex:
    // `INFO main: PROCESSMESSAGE: INVALID MESSAGESTART ;ersion peer=3`
    // `INFO main: ProcessMessages(version, 86 bytes): CHECKSUM ERROR nChecksum=67412de1 hdr.nChecksum=ddca6880`
    // which indicates the message was recognised as invalid).

    let mut rng = seeded_rng();

    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection).start().await;

    for _ in 0..ITERATIONS {
        let mut peer_stream = TcpStream::connect(node.addr()).await.unwrap();
        let version =
            Message::Version(Version::new(node.addr(), peer_stream.local_addr().unwrap()));
        let corrupted_version = corrupt_message(&mut rng, &version);

        // Send corrupt Version in place of Verack.
        // Contains header + message.
        let _ = peer_stream.write_all(&corrupted_version).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

#[tokio::test]
async fn corrupted_version_during_handshake_responder_side() {
    // ZG-RESISTANCE-002 (part 4)
    //
    // This particular case is considered alone because it is at particular risk of causing
    // troublesome behaviour, as seen with the valid metadata fuzzing against zebra.
    //
    // zebra: sends a verack before disconnecting (though somewhat slow running).
    // zcashd: logs suggest the message was ignored but the node doesn't disconnect.

    let mut rng = seeded_rng();

    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection).start().await;

    for _ in 0..ITERATIONS {
        let mut peer_stream = initiate_version_exchange(node.addr()).await.unwrap();

        let version_to_corrupt =
            Message::Version(Version::new(node.addr(), peer_stream.local_addr().unwrap()));
        let corrupted_version = corrupt_message(&mut rng, &version_to_corrupt);

        // Send corrupt Version in place of Verack.
        // Contains header + message.
        let _ = peer_stream.write_all(&corrupted_version).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

#[tokio::test]
async fn corrupted_version_when_node_initiates_handshake() {
    // ZG-RESISTANCE-003 (part 4, version only)
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
                let corrupted_version = {
                    let mut rng = peer_rng.write();
                    let version_to_corrupt = Message::Version(Version::new(
                        peer_stream.peer_addr().unwrap(),
                        peer_stream.local_addr().unwrap(),
                    ));
                    corrupt_message(&mut rng, &version_to_corrupt)
                };

                let _ = peer_stream.write_all(&corrupted_version).await;

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
async fn corrupted_version_inplace_of_verack_when_node_initiates_handshake() {
    // ZG-RESISTANCE-004 (part 4, version only)
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
                let version = Message::Version(Version::new(
                    peer_stream.peer_addr().unwrap(),
                    peer_stream.local_addr().unwrap(),
                ));
                version.write_to_stream(&mut peer_stream).await.unwrap();
                let verack = Message::read_from_stream(&mut peer_stream).await.unwrap();
                assert_matches!(verack, Message::Verack);

                // send bad version instead of verack
                let corrupted_version = {
                    let mut rng = peer_rng.write();
                    corrupt_message(&mut rng, &version)
                };
                let _ = peer_stream.write_all(&corrupted_version).await;

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
async fn corrupted_version_post_handshake() {
    // ZG-RESISTANCE-005 (part 4)
    //
    // This particular case is considered alone because it is at particular risk of causing
    // troublesome behaviour, as seen with the valid metadata fuzzing against zebra.
    //
    // zebra: disconnects
    // zcashd: disconnects for some messages, hangs for others.

    let mut rng = seeded_rng();
    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection).start().await;

    for _ in 0..ITERATIONS {
        let mut peer_stream = initiate_handshake(node.addr()).await.unwrap();

        let version_to_corrupt =
            Message::Version(Version::new(node.addr(), peer_stream.local_addr().unwrap()));
        let corrupted_version = corrupt_message(&mut rng, &version_to_corrupt);

        // Send corrupt Version in place of Verack.
        // Contains header + message.
        let _ = peer_stream.write_all(&corrupted_version).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

#[tokio::test]
async fn corrupted_messages_pre_handshake() {
    // ZG-RESISTANCE-001 (part 4)
    //
    // zebra: responds with a version before disconnecting (however, quite slow running).
    // zcashd: just ignores the message and doesn't disconnect.

    let test_messages = default_fuzz_messages();

    let mut rng = seeded_rng();
    let payloads = slightly_corrupted_messages(&mut rng, ITERATIONS, &test_messages);

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
async fn corrupted_messages_during_handshake_responder_side() {
    // ZG-RESISTANCE-002 (part 4)
    //
    // zebra: responds with verack before disconnecting (however, quite slow running).
    // zcashd: Some variants result in a terminated connect, some get ignored.

    let test_messages = default_fuzz_messages();

    let mut rng = seeded_rng();
    let payloads = slightly_corrupted_messages(&mut rng, ITERATIONS, &test_messages);

    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection).start().await;

    for payload in payloads {
        let mut peer_stream = initiate_version_exchange(node.addr()).await.unwrap();

        // Write the corrupted message in place of Verack.
        let _ = peer_stream.write_all(&payload).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

#[tokio::test]
async fn corrupted_messages_inplace_of_version_when_node_initiates_handshake() {
    // ZG-RESISTANCE-003 (part 4)
    //
    // zebra: disconnects immediately.
    // zcashd: Some messages get ignored and timeout.
    //
    // Note: zcashd is two orders of magnitude slower (~52 vs ~0.5 seconds)

    let test_messages = default_fuzz_messages();

    let mut rng = seeded_rng();
    let mut payloads = slightly_corrupted_messages(&mut rng, ITERATIONS, &test_messages);

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
async fn corrupted_messages_inplace_of_verack_when_node_initiates_handshake() {
    // ZG-RESISTANCE-004 (part 4)
    //
    // zebra: disconnects immediately.
    // zcashd: Some messages get ignored and timeout. Otherwise sends GetAddr, Ping and GetHeaders
    //         before disconnecting.
    //
    // Note: zcashd is two orders of magnitude slower (~52 vs ~0.5 seconds)

    let test_messages = default_fuzz_messages();

    let mut rng = seeded_rng();
    let mut payloads = slightly_corrupted_messages(&mut rng, ITERATIONS, &test_messages);

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
async fn corrupted_messages_post_handshake() {
    // ZG-RESISTANCE-005 (part 4)
    //
    // zebra: sends getdata and ignores message.
    // zcashd: disconnects for some messages, hangs for others.

    let test_messages = default_fuzz_messages();

    let mut rng = seeded_rng();
    let payloads = slightly_corrupted_messages(&mut rng, ITERATIONS, &test_messages);

    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection).start().await;

    for payload in payloads {
        let mut peer_stream = initiate_handshake(node.addr()).await.unwrap();

        // Write the corrupted message in place of Verack.
        let _ = peer_stream.write_all(&payload).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

// Corrupt messages from the supplied set by replacing a random number of bytes with random bytes.
pub fn slightly_corrupted_messages(
    rng: &mut ChaCha8Rng,
    n: usize,
    messages: &[Message],
) -> Vec<Vec<u8>> {
    (0..n)
        .map(|_| {
            let message = messages.choose(rng).unwrap();
            corrupt_message(rng, &message)
        })
        .collect()
}

fn corrupt_message(rng: &mut ChaCha8Rng, message: &Message) -> Vec<u8> {
    let mut message_buffer = vec![];
    let header = message.encode(&mut message_buffer).unwrap();
    let mut header_buffer = vec![];
    header.encode(&mut header_buffer).unwrap();

    let mut corrupted_header = corrupt_bytes(rng, &header_buffer);
    let mut corrupted_message = corrupt_bytes(rng, &message_buffer);

    corrupted_header.append(&mut corrupted_message);

    // Contains header + message.
    corrupted_header
}

fn corrupt_bytes(rng: &mut ChaCha8Rng, serialized: &[u8]) -> Vec<u8> {
    serialized
        .iter()
        .map(|byte| {
            if rng.gen_bool(CORRUPTION_PROBABILITY) {
                rng.gen()
            } else {
                *byte
            }
        })
        .collect()
}
