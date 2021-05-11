use crate::{
    helpers::{initiate_handshake, respond_to_handshake},
    protocol::{
        message::{Message, MessageFilter},
        payload::{
            addr::NetworkAddr,
            block::{Block, Headers, LocatorHashes},
            inv::{InvHash, ObjectKind},
            reject::CCode,
            Addr, Hash, Inv, Nonce, Version,
        },
    },
    setup::{
        config::{new_local_addr, read_config_file},
        node::{Action, Node},
    },
    wait_until,
};

use tokio::{
    net::TcpListener,
    time::{timeout, Duration},
};

#[tokio::test]
async fn ping_pong() {
    let node_meta = read_config_file();

    let listener = TcpListener::bind(new_local_addr()).await.unwrap();

    // Create a node and set the listener as an initial peer.
    let mut node = Node::new(node_meta);
    node.initial_peers(vec![listener.local_addr().unwrap()])
        .start()
        .await;

    // Receive the connection and perform the handshake once the node is started.
    let mut peer_stream = respond_to_handshake(listener).await.unwrap();

    let nonce = Nonce::default();
    Message::Ping(nonce)
        .write_to_stream(&mut peer_stream)
        .await
        .unwrap();

    wait_until!(10, {
        // Ignore queries from the node.
        let auto_responder = MessageFilter::with_all_auto_reply();
        if let Ok(Message::Pong(returned_nonce)) =
            auto_responder.read_from_stream(&mut peer_stream).await
        {
            // We received a pong and the nonce matches.
            assert_eq!(nonce, returned_nonce);
            true
        } else {
            // We didn't receive a pong.
            false
        }
    });

    node.stop().await;
}

#[tokio::test]
async fn reject_invalid_messages() {
    // ZG-CONFORMANCE-008
    //
    // The node rejects handshake and bloom filter messages post-handshake.
    //
    // The following messages should be rejected post-handshake:
    //
    //      Version     (Duplicate)
    //      Verack      (Duplicate)
    //      Inv         (Invalid -- with multiple advertised blocks)
    //      FilterLoad  (Obsolete)
    //      FilterAdd   (Obsolete)
    //      FilterClear (Obsolete)
    //
    // Test procedure:
    //      For each test message:
    //
    //      1. Connect and complete the handshake
    //      2. Send the test message
    //      3. Filter out all node queries
    //      4. Receive `Reject(kind)`
    //      5. Assert that `kind` is appropriate for the test message
    //
    // This test currently fails as neither Zebra nor ZCashd currently fully comply
    // with this behaviour, so we may need to revise our expectations.
    //
    // TODO: confirm expected behaviour.
    //
    // Current behaviour (if we initiate the connection):
    //  ZCashd:
    //      Version: works as expected
    //      Verack:  message is completely ignored
    //
    //  Zebra:
    //      Both Version and Verack result in a terminated connection

    let node_meta = read_config_file();

    let mut node = Node::new(node_meta);
    node.initial_action(Action::WaitForConnection(new_local_addr()))
        .start()
        .await;

    // list of test messages and their expected Reject kind
    let cases = vec![
        (
            Message::Version(Version::new(node.addr(), new_local_addr())),
            CCode::Duplicate,
        ),
        (Message::Verack, CCode::Duplicate),
        // TODO: rest of the message types once available
        // (Message::Inv(inv), CCode::Invalid),
        // (Message::FilterLoad, CCode::Obsolete),
        // (Message::FilterAdd, CCode::Obsolete),
        // (Message::FilterClear, CCode::Obsolete),
    ];

    let filter = MessageFilter::with_all_auto_reply().enable_logging();

    for (test_message, expected_ccode) in cases {
        let mut stream = initiate_handshake(node.addr()).await.unwrap();

        test_message.write_to_stream(&mut stream).await.unwrap();

        // Expect a Reject(Invalid) message
        match filter.read_from_stream(&mut stream).await.unwrap() {
            Message::Reject(reject) if reject.ccode == expected_ccode => {}
            message => panic!("Expected Reject(Invalid), but got: {:?}", message),
        }
    }
}

#[tokio::test]
async fn ignores_unsolicited_responses() {
    // ZG-CONFORMANCE-009
    //
    // The node ignore certain unsolicited messages but doesn’t disconnect.
    //
    // Messages to be tested: Reject, NotFound, Pong, Tx, Block, Header, Addr.
    //
    // Test procedure:
    //      Complete handshake, and then for each test message:
    //
    //      1. Send the message
    //      2. Send a ping request
    //      3. Receive a pong response

    let node_meta = read_config_file();

    let listener = TcpListener::bind(new_local_addr()).await.unwrap();

    // Create a node and set the listener as an initial peer.
    let mut node = Node::new(node_meta);
    node.initial_peers(vec![listener.local_addr().unwrap()])
        .start()
        .await;

    let mut stream = crate::helpers::respond_to_handshake(listener)
        .await
        .unwrap();

    // TODO: rest of the message types
    let test_messages = vec![
        Message::Pong(Nonce::default()),
        Message::Headers(Headers::empty()),
        Message::Addr(Addr::empty()),
        // Block(Block),
        // NotFound(Inv),
        // Tx(Tx),
    ];

    let filter = MessageFilter::with_all_auto_reply().enable_logging();

    for message in test_messages {
        message.write_to_stream(&mut stream).await.unwrap();

        let nonce = Nonce::default();
        Message::Ping(nonce)
            .write_to_stream(&mut stream)
            .await
            .unwrap();

        match filter.read_from_stream(&mut stream).await.unwrap() {
            Message::Pong(returned_nonce) => assert_eq!(nonce, returned_nonce),
            msg => panic!("Expected pong: {:?}", msg),
        }
    }

    node.stop().await;
}

#[tokio::test]
async fn eagerly_crawls_network_for_peers() {
    // ZG-CONFORMANCE-012
    //
    // The node crawls the network for new peers and eagerly connects.
    //
    // Test procedure:
    //
    //  1. Create a set of peer nodes, listening concurrently
    //  2. Connect to node with another main peer node
    //  3. Wait for `GetAddr`
    //  4. Send set of peer listener node addresses
    //  5. Expect the node to connect to each peer in the set
    //
    // This test currently fails for zcashd; zebra passes (with a caveat).
    //
    // Current behaviour:
    //
    //  zcashd: Has different behaviour depending on connection direction.
    //          If we initiate the main connection it sends Ping, GetHeaders,
    //          but never GetAddr.
    //          If the node initiates then it does send GetAddr, but it never connects
    //          to the peers.
    //
    // zebra:   Passes with flying colors, so long as we keep responding on the main connection.
    //          If we do not keep responding, then the peer connections take really long to establish,
    //          sometimes even spuriously failing the test completely.

    let node_meta = read_config_file();

    // create tcp listeners for peer set (port is only assigned on tcp bind)
    let mut listeners = Vec::new();
    for _ in 0u8..5 {
        listeners.push(TcpListener::bind(new_local_addr()).await.unwrap());
    }

    // get list of peer addresses
    let peer_addresses = listeners
        .iter()
        .map(|listener| listener.local_addr().unwrap())
        .collect::<Vec<_>>();

    // start the node
    let mut node = Node::new(node_meta);
    node.initial_action(Action::WaitForConnection(new_local_addr()))
        .start()
        .await;

    // start peer listeners which "pass" once they've accepted a connection, and
    // "fail" if the timeout expires. Timeout must be quite long, seems to take around
    // 20-60 seconds for zebra.
    let mut peer_handles = Vec::with_capacity(listeners.len());
    for listener in listeners {
        peer_handles.push(tokio::time::timeout(
            tokio::time::Duration::from_secs(120),
            tokio::spawn(async move {
                respond_to_handshake(listener).await.unwrap();
            }),
        ));
    }

    // connect to the node main
    let mut stream = initiate_handshake(node.addr()).await.unwrap();

    // wait for the `GetAddr`, filter out all other queries.
    let filter = MessageFilter::with_all_auto_reply()
        .enable_logging()
        .with_getaddr_filter(crate::protocol::message::Filter::Disabled);

    // reply with list of peer addresses
    match filter.read_from_stream(&mut stream).await.unwrap() {
        Message::GetAddr => {
            let peers = peer_addresses
                .iter()
                .map(|addr| NetworkAddr::new(*addr))
                .collect::<Vec<_>>();

            Message::Addr(Addr::new(peers))
                .write_to_stream(&mut stream)
                .await
                .unwrap();
        }
        message => panic!("Expected Message::GetAddr, but got {:?}", message),
    }

    // turn waiting for peer futures into a single future
    let wait_for_peers = tokio::spawn(async move {
        for handle in peer_handles {
            handle.await.unwrap().unwrap();
        }
    });

    // We need to keep responding to ping requests on the main connection,
    // otherwise it may get marked as unreliable (and the peer list gets ignored).
    //
    // Without this, zebra takes forever to connect and spuriously fails as well.
    // TBC - this is all speculation.
    let main_responder = tokio::spawn(async move {
        let filter = MessageFilter::with_all_auto_reply().enable_logging();

        // we don't expect to receive any messages
        let message = filter.read_from_stream(&mut stream).await.unwrap();
        panic!(
            "Unexpected message received by main connection: {:?}",
            message
        );
    });

    // wait for peer connections to complete, or main connection to break
    tokio::select! {
        result = main_responder => result.unwrap(),
        result = wait_for_peers => result.unwrap(),
    }

    node.stop().await;
}

// #[tokio::test]
async fn correctly_lists_peers() {
    // ZG-CONFORMANCE-013
    //
    // The node responds to a `GetAddr` with a list of peers it’s connected to. This command
    // should only be sent once, and by the node initiating the connection.
    //
    // Test procedure
    //  Start a node, and sequentially for each peer `i` of `N`:
    //      1. Initiate a connection and complete the handshake
    //      2. Send `GetAddr` request
    //      3. Receive `Addr` response
    //      4. Verify `Addr` contains list of previous `i-1` peers
    //
    // This test currently fails for zcashd and zebra passes.
    //
    // Current behaviour:
    //
    //  zcashd: Never responds. Logs indicate `Unknown command "getaddr" from peer=1` if we initiate
    //          the connection. If the node initiates the connection then the command is recoginized,
    //          but likely ignored (because only the initiating node is supposed to send it).
    //
    //  zebra:  Infinitely spams `GetAddr` and `GetData`. Can be coaxed into responding correctly if
    //          all its peer connections have responded to `GetAddr` with a non-empty list.

    let node_meta = read_config_file();

    // Create a node and main connection
    let mut node = Node::new(node_meta);
    node.initial_action(Action::WaitForConnection(new_local_addr()))
        .start()
        .await;

    let mut peers = Vec::new();

    for i in 0u8..5 {
        let mut new_peer = initiate_handshake(node.addr()).await.unwrap();
        let filter = MessageFilter::with_all_auto_reply().enable_logging();

        Message::GetAddr
            .write_to_stream(&mut new_peer)
            .await
            .unwrap();

        match filter.read_from_stream(&mut new_peer).await {
            Ok(Message::Addr(addresses)) => {
                // We need to sort the lists so we can compare them
                let mut expected = peers
                    .iter()
                    .map(|p: &tokio::net::TcpStream| p.local_addr().unwrap())
                    .collect::<Vec<_>>();
                expected.sort_unstable();

                let mut node_peers = addresses.iter().map(|net| net.addr).collect::<Vec<_>>();
                node_peers.sort_unstable();

                assert_eq!(expected, node_peers, "Testing node {}", i);
            }
            result => panic!("Peer {}: expected Ok(Addr), but got {:?}", i, result),
        }

        // list updated after check since current peer is not expecting to be part of the node's peer list
        peers.push(new_peer);
    }

    node.stop().await;
}

#[tokio::test]
async fn get_blocks() {
    // ZG-CONFORMANCE-015
    //
    // The node responds to `GetBlocks` requests with a list of blocks based on the provided range.
    //
    // We test the following conditions:
    //  1. unlimited queries i.e. stop_hash = 0
    //  2. range queries i.e. stop_hash = i
    //  3. a forked chain (we submit a valid hash, followed by incorrect hashes)
    //
    // Test procedure:
    //  1. Create a node and seed it with the testnet chain
    //  2. Establish a peer node
    //  3. For each test case:
    //      a) send GetBlocks
    //      b) receive Inv
    //      c) assert Inv received matches expectations
    //
    // The test currently fails for both Zebra and zcashd.
    //
    // Current behaviour:
    //
    //  zcashd: Fails because it ignores requests when the last hash is the last hash it knows of. We expect it to return
    //          an empty Inv.
    //
    //  zebra: does not support seeding as yet, and therefore cannot perform this test.
    //
    // Note: zcashd also excludes the `stop_hash` from the range, whereas the spec states that it should be inclusive.
    //       We are taking current behaviour as correct.

    let (zig, node_meta) = read_config_file();

    // Create a node with knowledge of the initial three testnet blocks
    let mut node = Node::new(node_meta);
    node.initial_action(Action::SeedWithTestnetBlocks {
        socket_addr: zig.new_local_addr(),
        block_count: 3,
    })
    .log_to_stdout(true)
    .start()
    .await;

    let blocks = Block::initial_testnet_blocks();

    let mut stream = initiate_handshake(node.addr()).await.unwrap();
    let filter = MessageFilter::with_all_auto_reply();

    // Test unlimited range queries, where given the hash for block i we expect all
    // of its children as a reply.
    // i.e. GetBlocks(i) -> Inv(i+1..)
    for (i, block) in blocks.iter().enumerate() {
        Message::GetBlocks(LocatorHashes::new(
            vec![block.double_sha256().unwrap()],
            Hash::zeroed(),
        ))
        .write_to_stream(&mut stream)
        .await
        .unwrap();

        match filter.read_from_stream(&mut stream).await.unwrap() {
            Message::Inv(inv) => {
                // collect inventory hashes for all blocks after i (i's children)
                let inv_hashes = blocks.iter().skip(i + 1).map(|b| b.inv_hash()).collect();
                let expected = Inv::new(inv_hashes);
                assert_eq!(inv, expected);
            }
            message => panic!("Expected Inv, but got {:?}", message),
        }
    }

    // Test `hash_stop` (it should be included in the range, but zcashd excludes it -- see note).
    Message::GetBlocks(LocatorHashes::new(
        vec![blocks[0].double_sha256().unwrap()],
        blocks[2].double_sha256().unwrap(),
    ))
    .write_to_stream(&mut stream)
    .await
    .unwrap();
    match filter.read_from_stream(&mut stream).await.unwrap() {
        Message::Inv(inv) => {
            let expected = Inv::new(vec![blocks[1].inv_hash()]);
            assert_eq!(inv, expected);
        }
        message => panic!("Expected Inv, but got {:?}", message),
    }

    // Test that we get corrected if we are "off chain".
    // We expect that unknown hashes get ignored, until it finds a known hash; it then returns
    // all known children of that block.
    let locators = LocatorHashes::new(
        vec![
            blocks[1].double_sha256().unwrap(),
            Hash::new([19; 32]),
            Hash::new([22; 32]),
        ],
        Hash::zeroed(),
    );
    Message::GetBlocks(locators)
        .write_to_stream(&mut stream)
        .await
        .unwrap();
    match filter.read_from_stream(&mut stream).await.unwrap() {
        Message::Inv(inv) => {
            let expected = Inv::new(vec![blocks[2].inv_hash()]);
            assert_eq!(inv, expected);
        }
        message => panic!("Expected Inv, but got {:?}", message),
    }

    node.stop().await;
}

#[tokio::test]
async fn correctly_lists_blocks() {
    // ZG-CONFORMANCE-016
    //
    // The node responds to `GetHeaders` request with a list of block headers based on the provided range.
    //
    // We test the following conditions:
    //  1. unlimited queries i.e. stop_hash = 0
    //  2. range queries i.e. stop_hash = i
    //  3. a forked chain (we submit a header which doesn't match the chain)
    //
    // Test procedure:
    //  1. Create a node and seed it with the testnet chain
    //  2. Establish a peer node
    //  3. For each test case:
    //      a) send GetHeaders
    //      b) receive Headers
    //      c) assert headers received match expectations
    //
    // The test currently fails for both Zebra and zcashd.
    //
    // Current behaviour:
    //
    //  zcashd: Fails for range queries where the head of the chain equals the stop hash. We expect to receive an empty set,
    //          but instead we get header [i+1] (which exceeds stop_hash).
    //
    //  zebra: does not support seeding as yet, and therefore cannot perform this test.

    let node_meta = read_config_file();

    // Create a node with knowledge of the initial three testnet blocks
    let mut node = Node::new(node_meta);
    node.initial_action(Action::SeedWithTestnetBlocks {
        socket_addr: new_local_addr(),
        block_count: 3,
    })
    .start()
    .await;

    // block headers and hashes
    let expected = Block::initial_testnet_blocks()
        .iter()
        .map(|block| block.header.clone())
        .collect::<Vec<_>>();
    let hashes = expected
        .iter()
        .map(|header| header.double_sha256().unwrap())
        .collect::<Vec<_>>();

    // locator hashes are stored in reverse order
    let locator = vec![
        vec![hashes[0].clone()],
        vec![hashes[1].clone(), hashes[0].clone()],
        vec![hashes[2].clone(), hashes[1].clone(), hashes[0].clone()],
    ];

    // Establish a peer node
    let mut stream = initiate_handshake(node.addr()).await.unwrap();
    let filter = MessageFilter::with_all_auto_reply();

    // Query for all blocks from i onwards (stop_hash = [0])
    for i in 0..expected.len() {
        Message::GetHeaders(LocatorHashes::new(locator[i].clone(), Hash::zeroed()))
            .write_to_stream(&mut stream)
            .await
            .unwrap();

        match filter.read_from_stream(&mut stream).await.unwrap() {
            Message::Headers(headers) => assert_eq!(
                headers.headers,
                expected[(i + 1)..],
                "test for Headers([{}..])",
                i
            ),
            messsage => panic!("Expected Headers, but got: {:?}", messsage),
        }
    }

    // Query for all possible valid ranges
    let ranges: Vec<(usize, usize)> = vec![(0, 0), (0, 1), (0, 2), (1, 1), (1, 2), (2, 2)];
    for (start, stop) in ranges {
        Message::GetHeaders(LocatorHashes::new(
            locator[start].clone(),
            hashes[stop].clone(),
        ))
        .write_to_stream(&mut stream)
        .await
        .unwrap();

        // We use start+1 because Headers should list the blocks starting *after* the
        // final location in GetHeaders, and up (and including) the stop-hash.
        match filter.read_from_stream(&mut stream).await.unwrap() {
            Message::Headers(headers) => assert_eq!(
                headers.headers,
                expected[start + 1..=stop],
                "test for Headers([{}..={}])",
                start + 1,
                stop
            ),
            messsage => panic!("Expected Headers, but got: {:?}", messsage),
        }
    }

    // Query as if from a fork. We replace [2], and expect to be corrected
    let mut fork_locator = locator[1].clone();
    fork_locator.insert(0, Hash::new([17; 32]));
    Message::GetHeaders(LocatorHashes::new(fork_locator, Hash::zeroed()))
        .write_to_stream(&mut stream)
        .await
        .unwrap();
    match filter.read_from_stream(&mut stream).await.unwrap() {
        Message::Headers(headers) => {
            assert_eq!(headers.headers, expected[2..], "test for forked Headers")
        }
        messsage => panic!("Expected Headers, but got: {:?}", messsage),
    }

    node.stop().await;
}

#[tokio::test]
async fn get_data_blocks() {
    // ZG-CONFORMANCE-017, blocks portion
    //
    // The node responds to `GetData` requests with the appropriate transaction or block as requested by the peer.
    //
    // We test the following conditions:
    //  1. query for i=1..3 blocks
    //  2. a non-existing block
    //  3. a mixture of existing and non-existing blocks
    //
    // Test procedure:
    //  1. Create a node and seed it with the testnet chain
    //  2. Establish a peer node
    //  3. For each test case:
    //      a) send GetData
    //      b) receive a series Blocks
    //      c) assert Block received matches expectations
    //
    // The test currently fails for both Zebra and zcashd.
    //
    // Current behaviour:
    //
    //  zcashd: Ignores non-existing block requests, we expect `NotFound` to be sent but it never does (both in cases 2 and 3).
    //
    //  zebra: does not support seeding as yet, and therefore cannot perform this test.

    let node_meta = read_config_file();

    // Create a node with knowledge of the initial three testnet blocks
    let mut node = Node::new(node_meta);
    node.initial_action(Action::SeedWithTestnetBlocks {
        socket_addr: new_local_addr(),
        block_count: 3,
    })
    .log_to_stdout(true)
    .start()
    .await;

    // block headers and hashes
    let blocks = vec![
        Box::new(Block::testnet_genesis()),
        Box::new(Block::testnet_1()),
        Box::new(Block::testnet_2()),
    ];

    let inv_blocks = blocks
        .iter()
        .map(|block| block.inv_hash())
        .collect::<Vec<_>>();

    // Establish a peer node
    let mut stream = initiate_handshake(node.addr()).await.unwrap();
    let filter = MessageFilter::with_all_auto_reply();

    // Query for the first i blocks
    for i in 0..blocks.len() {
        Message::GetData(Inv::new(inv_blocks[..=i].to_vec()))
            .write_to_stream(&mut stream)
            .await
            .unwrap();
        // Expect the i blocks
        for j in 0..=i {
            match filter.read_from_stream(&mut stream).await.unwrap() {
                Message::Block(block) => assert_eq!(block, blocks[j], "run {}, {}", i, j),
                messsage => panic!("Expected Block, but got: {:?}", messsage),
            }
        }
    }

    // Query for a non-existant block
    let non_existant = InvHash::new(ObjectKind::Block, Hash::new([17; 32]));
    let non_existant_inv = Inv::new(vec![non_existant.clone()]);
    Message::GetData(non_existant_inv.clone())
        .write_to_stream(&mut stream)
        .await
        .unwrap();
    match filter.read_from_stream(&mut stream).await.unwrap() {
        Message::NotFound(not_found) => assert_eq!(not_found, non_existant_inv),
        messsage => panic!("Expected NotFound, but got: {:?}", messsage),
    }

    // Query a mixture of existing and non-existing blocks
    let mut mixed_blocks = inv_blocks;
    mixed_blocks.insert(1, non_existant.clone());
    mixed_blocks.push(non_existant.clone());

    let expected = vec![
        Message::Block(Box::new(Block::testnet_genesis())),
        Message::NotFound(non_existant_inv.clone()),
        Message::Block(Box::new(Block::testnet_1())),
        Message::Block(Box::new(Block::testnet_2())),
        Message::NotFound(non_existant_inv),
    ];

    Message::GetData(Inv::new(mixed_blocks))
        .write_to_stream(&mut stream)
        .await
        .unwrap();

    for expect in expected {
        let message = filter.read_from_stream(&mut stream).await.unwrap();
        assert_eq!(message, expect);
    }

    node.stop().await;
}

#[allow(dead_code)]
async fn unsolicitation_listener() {
    let node_meta = read_config_file();

    let mut node = Node::new(node_meta);
    node.start().await;

    let mut peer_stream = initiate_handshake(node.addr()).await.unwrap();

    let auto_responder = MessageFilter::with_all_auto_reply().enable_logging();

    for _ in 0usize..10 {
        let result = timeout(
            Duration::from_secs(5),
            auto_responder.read_from_stream(&mut peer_stream),
        )
        .await;

        match result {
            Err(elapsed) => println!("Timeout after {}", elapsed),
            Ok(Ok(message)) => println!("Received unfiltered message: {:?}", message),
            Ok(Err(err)) => println!("Error receiving message: {:?}", err),
        }
    }

    node.stop().await;
}
