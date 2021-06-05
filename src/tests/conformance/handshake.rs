use crate::{
    helpers::{
        is_termination_error,
        synthetic_peers::{SyntheticNode, SyntheticNodeConfig},
    },
    protocol::{
        message::{
            filter::{Filter, MessageFilter},
            Message,
        },
        payload::{
            block::{Block, LocatorHashes},
            Addr, Hash, Inv, Nonce, Version,
        },
    },
    setup::{
        config::new_local_addr,
        node::{Action, Node},
    },
    wait_until,
};

use assert_matches::assert_matches;
use tokio::net::{TcpListener, TcpStream};

// Default timeout for connection reads in seconds.
const TIMEOUT: u64 = 10;

#[tokio::test]
async fn handshake_responder_side() {
    // ZG-CONFORMANCE-001

    // Spin up a node instance.
    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection(new_local_addr()))
        .start()
        .await;

    // Create a synthetic node and enable handshaking.
    let synthetic_node = SyntheticNode::new(SyntheticNodeConfig {
        enable_handshaking: true,
        ..Default::default()
    })
    .await
    .unwrap();

    // Connect to the node and initiate the handshake.
    synthetic_node.connect(node.addr()).await.unwrap();

    // This is only set post-handshake (if enabled).
    assert!(synthetic_node.is_connected(node.addr()));

    // Gracefully shut down the nodes.
    synthetic_node.shut_down();
    node.stop().await;
}

#[tokio::test]
async fn handshake_initiator_side() {
    // ZG-CONFORMANCE-002
    use crate::helpers::enable_tracing;
    enable_tracing();

    // Create a synthetic node and enable handshaking.
    let synthetic_node = SyntheticNode::new(SyntheticNodeConfig {
        enable_handshaking: true,
        ..Default::default()
    })
    .await
    .unwrap();

    // Spin up a node and set the synthetic node as an initial peer.
    let mut node: Node = Default::default();
    node.initial_peers(vec![synthetic_node.listening_addr()])
        .start()
        .await;

    // Check the connection has been established (this is only set post-handshake). We can't check
    // for the addr as nodes use ephemeral addresses when initiating connections.
    wait_until!(5, synthetic_node.num_connected() == 1);

    // Gracefully shut down the nodes.
    synthetic_node.shut_down();
    node.stop().await;
}

#[tokio::test]
async fn ignore_non_version_before_handshake() {
    // ZG-CONFORMANCE-003
    //
    // The node should ignore non-Version messages before the handshake has been performed.
    //
    // zebra: eagerly sends version but doesn't respnd to verack and disconnects.
    // zcashd: ignores the message and completes the handshake.

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

    // Spin up a node instance.
    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection(new_local_addr()))
        .start()
        .await;

    // Configuration to be used by all synthetic nodes, no handshaking, no message filters.
    let config: SyntheticNodeConfig = Default::default();

    for message in test_messages {
        let mut synthetic_node = SyntheticNode::new(config.clone()).await.unwrap();

        // Connect to the node, don't handshake.
        synthetic_node.connect(node.addr()).await.unwrap();

        // Send a non-version message.
        synthetic_node
            .send_direct_message(node.addr(), message)
            .await
            .unwrap();

        // Expect the node to ignore the previous message, verify by completing the handshake.
        // Send Version.
        synthetic_node
            .send_direct_message(
                node.addr(),
                Message::Version(Version::new(synthetic_node.listening_addr(), node.addr())),
            )
            .await
            .unwrap();

        // Read Version.
        let (_, version) = synthetic_node.recv_message_timeout(TIMEOUT).await.unwrap();
        assert_matches!(version, Message::Version(..));

        // Send Verack.
        synthetic_node
            .send_direct_message(node.addr(), Message::Verack)
            .await
            .unwrap();

        // Read Verack.
        let (_, verack) = synthetic_node.recv_message_timeout(TIMEOUT).await.unwrap();
        assert_matches!(verack, Message::Verack);

        // Gracefully shut down the synthetic node.
        synthetic_node.shut_down();
    }

    // Gracefully shut down the node.
    node.stop().await;
}

#[tokio::test]
async fn ignore_non_version_replies_to_version() {
    // ZG-CONFORMANCE-004
    //
    // The node should ignore non-Version messages in response to the initial Version it sent.
    //
    // Due to how we instrument the test node, we need to have the list of peers ready when we start the node.
    //
    // zebra: doesn't respond to verack and disconnects.
    // zcashd: ignores the message and completes the handshake.

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

    // Configuration to be used by all synthetic nodes, no handshaking, no message filters.
    let config: SyntheticNodeConfig = Default::default();
    // Instantiate a node instance without starting it (so we have access to its addr).
    let mut node: Node = Default::default();
    let node_addr = node.addr();

    // Store the listening addresses of the synthetic nodes so they can be set as initial peers of
    // the node.
    let mut listeners = Vec::with_capacity(test_messages.len());

    // Create a future for each message.
    let mut handles = Vec::with_capacity(test_messages.len());

    for message in test_messages {
        // Create a synthetic node and store its address.
        let mut synthetic_node = SyntheticNode::new(config.clone()).await.unwrap();
        listeners.push(synthetic_node.listening_addr());

        let handle = tokio::spawn(async move {
            // Receive Version.
            let (source, version) = synthetic_node.recv_message_timeout(TIMEOUT).await.unwrap();
            assert_matches!(version, Message::Version(..));

            // Send non-version.
            synthetic_node
                .send_direct_message(source, message)
                .await
                .unwrap();

            // Initiate the handshake by sending a Version.
            synthetic_node
                .send_direct_message(
                    source,
                    Message::Version(Version::new(synthetic_node.listening_addr(), node_addr)),
                )
                .await
                .unwrap();

            // Receiving a Verack indicates the non-version message was ignored.
            let (_, verack) = synthetic_node.recv_message_timeout(TIMEOUT).await.unwrap();
            assert_matches!(verack, Message::Verack);

            // Gracefully shut down the synthetic node.
            synthetic_node.shut_down();
        });

        handles.push(handle);
    }

    // Start the node instance with the initial peers.
    node.initial_peers(listeners).start().await;

    // Run each future to completion.
    for handle in handles {
        handle.await.unwrap();
    }

    // Gracefully shut down the node.
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
    // Current behaviour (if we initiate the connection):
    //  ZCashd:
    //      1. We send `version` with an obsolete version number
    //      2. Node sends `Reject(Obsolete)`
    //      3. Node sends `Ping` (this is unexpected)
    //      4. Node sends `GetHeaders` (this is unexpected)
    //      5. Node terminates the connection
    //
    //  Zebra:
    //      1. We send `version` with an obsolete version number
    //      2. Node sends `version`
    //      3. Node sends `verack`
    //      4. Node terminates the connection (no `Reject(Obsolete)` sent)

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
