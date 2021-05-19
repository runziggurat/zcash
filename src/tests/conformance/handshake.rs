use crate::{
    assert_matches,
    helpers::is_termination_error,
    protocol::{
        message::{Filter, Message, MessageFilter},
        payload::{
            block::{Block, LocatorHashes},
            Addr, Hash, Inv, Nonce, Version,
        },
    },
    setup::{
        config::new_local_addr,
        node::{Action, Node},
    },
};

use tokio::net::{TcpListener, TcpStream};

#[tokio::test]
async fn handshake_responder_side() {
    // ZG-CONFORMANCE-001

    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection(new_local_addr()))
        .start()
        .await;

    // Connect to the node and initiate handshake.
    let mut peer_stream = TcpStream::connect(node.addr()).await.unwrap();

    Message::Version(Version::new(node.addr(), peer_stream.local_addr().unwrap()))
        .write_to_stream(&mut peer_stream)
        .await
        .unwrap();

    let version = Message::read_from_stream(&mut peer_stream).await.unwrap();
    assert_matches!(version, Message::Version(..));

    Message::Verack
        .write_to_stream(&mut peer_stream)
        .await
        .unwrap();

    let verack = Message::read_from_stream(&mut peer_stream).await.unwrap();
    assert_matches!(verack, Message::Verack);

    node.stop().await;
}

#[tokio::test]
async fn handshake_initiator_side() {
    // ZG-CONFORMANCE-002

    let listener = TcpListener::bind(new_local_addr()).await.unwrap();

    // Create a node and set the listener as an initial peer.
    let mut node: Node = Default::default();
    node.initial_peers(vec![listener.local_addr().unwrap()])
        .start()
        .await;

    // Expect the node to initiate the handshake.
    let (mut peer_stream, addr) = listener.accept().await.unwrap();
    let version = Message::read_from_stream(&mut peer_stream).await.unwrap();
    assert_matches!(version, Message::Version(..));

    Message::Version(Version::new(addr, listener.local_addr().unwrap()))
        .write_to_stream(&mut peer_stream)
        .await
        .unwrap();

    let verack = Message::read_from_stream(&mut peer_stream).await.unwrap();
    assert_matches!(verack, Message::Verack);

    Message::Verack
        .write_to_stream(&mut peer_stream)
        .await
        .unwrap();

    node.stop().await;
}

#[tokio::test]
async fn reject_non_version_before_handshake() {
    // ZG-CONFORMANCE-003
    //
    // The node should reject non-Version messages before the handshake has been performed.
    //
    // A node can react in one of the following ways:
    //
    //  a) the message is ignored
    //  b) the connection is terminated
    //  c) responds to our message
    //  d) becomes unersponsive to future communications
    //
    // of which only (a) and (b) are valid responses. This test operates in the following manner:
    //
    // for each non-version message:
    //
    //  1. connect to the node
    //  2. send the message
    //  3. send the version message
    //  4. receive version
    //  5. receive verack
    //
    // We expect the following to occur for each of the possible node reactions:
    //
    //  a) (2) is ignored so we expect to complete the handshake - (3,4,5) should succeed
    //  b) The connection should terminate after the node has processed (2), which implies (3) may or may not
    //      succeed depending on the timing. The node may also already have sent its `version` eagerly, so
    //      (4) may also succeed or fail. (5) will definitely fail.
    //  c) Messages received in (4, 5) will not match (version, verack)
    //  d) steps (3, 4) or (5) cause time out

    let genesis_block = Block::testnet_genesis();
    let block_hash = genesis_block.double_sha256().unwrap();
    let block_inv = Inv::new(vec![genesis_block.inv_hash()]);
    let block_loc = LocatorHashes::new(vec![block_hash], Hash::zeroed());
    let test_messages = vec![
        Message::GetAddr,
        Message::MemPool,
        Message::Verack,
        Message::Ping(Nonce::default()),
        Message::Pong(Nonce::default()),
        Message::GetAddr,
        Message::Addr(Addr::empty()),
        Message::GetHeaders(block_loc.clone()),
        Message::GetBlocks(block_loc),
        Message::GetData(block_inv.clone()),
        Message::GetData(Inv::new(vec![genesis_block.txs[0].inv_hash()])),
        Message::Inv(block_inv.clone()),
        Message::NotFound(block_inv),
    ];

    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection(new_local_addr()))
        .start()
        .await;

    for message in test_messages {
        // (1) connect to node
        let mut stream = TcpStream::connect(node.addr()).await.unwrap();

        // (2) send non-version message
        message.write_to_stream(&mut stream).await.unwrap();

        // (3) send version message
        match Message::Version(Version::new(node.addr(), stream.local_addr().unwrap()))
            .write_to_stream(&mut stream)
            .await
        {
            Ok(_) => {}
            Err(err) if is_termination_error(&err) => continue,
            Err(err) => panic!("Unexpected error while sending version: {:?}", err),
        };

        // (4) read version
        match Message::read_from_stream(&mut stream).await {
            Ok(message) => assert_matches!(message, Message::Version(..)),
            Err(err) if is_termination_error(&err) => continue,
            Err(err) => panic!("Unexpected error while receiving version: {:?}", err),
        };

        // (5) read verack
        match Message::read_from_stream(&mut stream).await {
            Ok(message) => assert_matches!(message, Message::Verack),
            Err(err) if is_termination_error(&err) => continue,
            Err(err) => panic!("Unexpected error while receiving verack: {:?}", err),
        }
    }

    node.stop().await;
}

#[tokio::test]
async fn reject_non_version_replies_to_version() {
    // ZG-CONFORMANCE-004
    //
    // The node should reject non-Version messages in response to the initial Version it sent.
    //
    // A node can react in one of the following ways:
    //
    //  a) the message is ignored
    //  b) the connection is terminated
    //  c) responds to our message
    //  d) becomes unersponsive to future communications
    //
    // of which only (a) and (b) are valid responses. This test operates in the following manner:
    //
    // For each non-version message, create a peer node and
    //
    //  1) wait for the incoming `version` message
    //  2) send a non-version message
    //  3) send the version message
    //  4) receive a response
    //
    // We expect the following to occur for each of the possible node reactions:
    //
    //  a) (2) is ignored, therefore (3) should succeed, and (4) should be `verack`
    //  b) Node terminates the connection upon processing the message sent in (2),
    //     so either step (3) or at latest (4) should fail (timing dependent on node)
    //  c) message received in (4) is not `verack`
    //  d) steps (3) or (4) cause time out
    //
    // Due to how we instrument the test node, we need to have the list of peers ready when we start the node.
    // This implies we need each test message to operate on a separate connection concurrently.

    let genesis_block = Block::testnet_genesis();
    let block_hash = genesis_block.double_sha256().unwrap();
    let block_inv = Inv::new(vec![genesis_block.inv_hash()]);
    let block_loc = LocatorHashes::new(vec![block_hash], Hash::zeroed());
    let mut test_messages = vec![
        Message::GetAddr,
        Message::MemPool,
        Message::Verack,
        Message::Ping(Nonce::default()),
        Message::Pong(Nonce::default()),
        Message::GetAddr,
        Message::Addr(Addr::empty()),
        Message::GetHeaders(block_loc.clone()),
        Message::GetBlocks(block_loc),
        Message::GetData(block_inv.clone()),
        Message::GetData(Inv::new(vec![genesis_block.txs[0].inv_hash()])),
        Message::Inv(block_inv.clone()),
        Message::NotFound(block_inv),
    ];

    // Create and bind TCP listeners (so we have the ports ready for instantiating the node)
    let mut listeners = Vec::with_capacity(test_messages.len());
    for _ in test_messages.iter() {
        listeners.push(TcpListener::bind(new_local_addr()).await.unwrap());
    }

    let addrs = listeners
        .iter()
        .map(|listener| listener.local_addr().unwrap())
        .collect();
    let mut node: Node = Default::default();
    node.initial_peers(addrs);

    let mut handles = Vec::with_capacity(test_messages.len());

    // create and start a future for each test message
    for _ in 0..test_messages.len() {
        let listener = listeners.pop().unwrap();
        let message = test_messages.pop().unwrap();

        handles.push(tokio::spawn(async move {
            let (mut stream, addr) = listener.accept().await.unwrap();

            // (1) receive incoming `version`
            let version = Message::read_from_stream(&mut stream).await.unwrap();
            assert_matches!(version, Message::Version(..));

            // (2) send non-version message
            message.write_to_stream(&mut stream).await.unwrap();

            // (3) send `version` to start our end of the handshake
            match Message::Version(Version::new(addr, listener.local_addr().unwrap()))
                .write_to_stream(&mut stream)
                .await
            {
                Ok(_) => {}
                Err(err) if is_termination_error(&err) => return,
                Err(err) => panic!("Unexpected error while sending version: {:?}", err),
            }

            // (4) receive `verack` in response to our `version`
            match Message::read_from_stream(&mut stream).await {
                Ok(message) => assert_matches!(message, Message::Verack),
                Err(err) if is_termination_error(&err) => {}
                Err(err) => panic!("Unexpected error while receiving verack: {:?}", err),
            }
        }));
    }

    node.start().await;

    for handle in handles {
        handle.await.unwrap();
    }

    node.stop().await;
}

#[tokio::test]
async fn reject_non_verack_replies_to_verack() {
    // Conformance test 005.
    //
    // The node rejects non-Verack message as a response to initial Verack it sent.
    //
    // Test procedure:
    //  For each non-verack message,
    //
    //  1. Expect `Version`
    //  2. Send `Version`
    //  3. Expect `Verack`
    //  4. Send test message
    //  5. Expect `Reject(Invalid)`
    //  6. Expect connection to be terminated
    //
    // This test currently fails as neither Zebra nor ZCashd currently fully comply
    // with this behaviour, so we may need to revise our expectations.
    //
    // ZCashd node eagerly sends messages before handshake has been concluded.
    // Zebra node does not send Reject, but terminates the connection.
    //
    // TODO: confirm expected behaviour

    let genesis_block = Block::testnet_genesis();
    let block_hash = genesis_block.double_sha256().unwrap();
    let block_inv = Inv::new(vec![genesis_block.inv_hash()]);
    let block_loc = LocatorHashes::new(vec![block_hash], Hash::zeroed());
    let mut test_messages = vec![
        Message::Version(Version::new(new_local_addr(), new_local_addr())),
        Message::GetAddr,
        Message::MemPool,
        Message::Ping(Nonce::default()),
        Message::Pong(Nonce::default()),
        Message::GetAddr,
        Message::Addr(Addr::empty()),
        Message::GetHeaders(block_loc.clone()),
        Message::GetBlocks(block_loc),
        Message::GetData(block_inv.clone()),
        Message::GetData(Inv::new(vec![genesis_block.txs[0].inv_hash()])),
        Message::Inv(block_inv.clone()),
        Message::NotFound(block_inv),
    ];

    // Create and bind TCP listeners (so we have the ports ready for instantiating the node)
    let mut listeners = Vec::with_capacity(test_messages.len());
    for _ in test_messages.iter() {
        listeners.push(TcpListener::bind(new_local_addr()).await.unwrap());
    }

    let addrs = listeners
        .iter()
        .map(|listener| listener.local_addr().unwrap())
        .collect();
    let mut node: Node = Default::default();
    node.initial_peers(addrs);

    let mut handles = Vec::with_capacity(test_messages.len());

    // create and start a future for each test message
    for _ in 0..test_messages.len() {
        let listener = listeners.pop().unwrap();
        let message = test_messages.pop().unwrap();

        handles.push(tokio::spawn(async move {
            let (mut stream, addr) = listener.accept().await.unwrap();

            // (1) receive incoming `version`
            let version = Message::read_from_stream(&mut stream).await.unwrap();
            assert_matches!(version, Message::Version(..));

            // (2) send `version`
            Message::Version(Version::new(addr, listener.local_addr().unwrap()))
                .write_to_stream(&mut stream)
                .await
                .unwrap();

            // (3) receive `verack`
            let verack = Message::read_from_stream(&mut stream).await.unwrap();
            assert_matches!(verack, Message::Verack);

            // (4) send test message
            message.write_to_stream(&mut stream).await.unwrap();

            // (5) receive Reject(Invalid)
            let reject = Message::read_from_stream(&mut stream).await.unwrap();
            match reject {
                Message::Reject(reject) if reject.ccode.is_invalid() => {}
                reply => panic!("Expected Reject(Invalid), but got {:?}", reply),
            }

            // (6) check that connection has been terminated
            match Message::read_from_stream(&mut stream).await {
                Err(err) if is_termination_error(&err) => {}
                result => panic!("Expected terminated connection but got: {:?}", result),
            }
        }));
    }

    node.start().await;

    for handle in handles {
        handle.await.unwrap();
    }

    node.stop().await;
}

#[tokio::test]
async fn reject_version_reusing_nonce() {
    // ZG-CONFORMANCE-006
    //
    // The node rejects connections reusing its nonce (usually indicative of self-connection).
    //
    // 1. Wait for node to send version
    // 2. Send back version with same nonce
    // 3. Connection should be terminated

    let listener = TcpListener::bind(new_local_addr()).await.unwrap();

    let mut node: Node = Default::default();
    node.initial_peers(vec![listener.local_addr().unwrap()])
        .start()
        .await;

    let (mut stream, _) = listener.accept().await.unwrap();

    let version = match Message::read_from_stream(&mut stream).await.unwrap() {
        Message::Version(version) => version,
        message => panic!("Expected version but received: {:?}", message),
    };

    let mut bad_version = Version::new(node.addr(), stream.local_addr().unwrap());
    bad_version.nonce = version.nonce;
    Message::Version(bad_version)
        .write_to_stream(&mut stream)
        .await
        .unwrap();

    // This is required because the zcashd node eagerly sends `ping` and `getheaders` even though
    // our version message is broken.
    // TODO: tbd if this is desired behaviour or if this should fail the test.
    let filter = MessageFilter::with_all_disabled()
        .with_ping_filter(Filter::Enabled)
        .with_getheaders_filter(Filter::Enabled);

    match filter.read_from_stream(&mut stream).await {
        Err(err) if is_termination_error(&err) => {}
        result => panic!(
            "Expected terminated connection error, but received: {:?}",
            result
        ),
    }

    node.stop().await;
}

#[tokio::test]
async fn reject_obsolete_versions() {
    // ZG-CONFORMANCE-007
    //
    // The node rejects connections with obsolete node versions.
    //
    // We expect the following behaviour, regardless of who initiates the connection:
    //
    //  1. We send `version` with an obsolete version number
    //  2. The node responds with `Reject(Obsolete)`
    //  3. The node terminates the connection
    //
    // This test currently fails as neither Zebra nor ZCashd currently fully comply
    // with this behaviour, so we may need to revise our expectations.
    //
    // TODO: confirm expected behaviour.
    //
    // Current behaviour (if we initiate the connection):
    //  ZCashd:
    //      1. We send `version` with an obsolete version number
    //      2. Node sends `Reject(Obsolete)`
    //      3. Node sends `Ping`
    //      4. Node sends `GetHeaders`
    //      5. Node terminates the connection
    //
    //  Zebra:
    //      1. We send `version` with an obsolete version number
    //      2. Node sends `version`
    //      3. Node sends `verack`
    //      4. Node terminates the connection

    let obsolete_version_numbers: Vec<u32> = (170000..170002).collect();

    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection(new_local_addr()))
        .start()
        .await;

    for obsolete_version_number in obsolete_version_numbers {
        // open connection
        let mut stream = TcpStream::connect(node.addr()).await.unwrap();

        // send obsolete version
        let obsolete_version = Version::new(node.addr(), stream.local_addr().unwrap())
            .with_version(obsolete_version_number);
        Message::Version(obsolete_version)
            .write_to_stream(&mut stream)
            .await
            .unwrap();

        // expect Reject(Obsolete)
        match Message::read_from_stream(&mut stream).await.unwrap() {
            Message::Reject(reject) => assert!(reject.ccode.is_obsolete()),
            message => panic!("Expected Message::Reject(Obsolete), but got {:?}", message),
        }

        // check that connection has been terminated
        match Message::read_from_stream(&mut stream).await {
            Err(err) if is_termination_error(&err) => {}
            result => panic!("Expected terminated connection but got: {:?}", result),
        }
    }

    node.stop().await;
}
