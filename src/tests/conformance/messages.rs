use crate::{
    helpers::{
        autorespond_and_expect_disconnect, initiate_handshake, respond_to_handshake,
        synthetic_peers::SyntheticNode,
    },
    protocol::{
        message::{
            filter::{Filter, MessageFilter},
            Message,
        },
        payload::{
            addr::NetworkAddr,
            block::{Block, Headers, LocatorHashes},
            inv::{InvHash, ObjectKind},
            reject::CCode,
            Addr, FilterAdd, FilterLoad, Hash, Inv, Nonce, Version,
        },
    },
    setup::{
        config::new_local_addr,
        node::{Action, Node},
    },
};

use assert_matches::assert_matches;
use tokio::{
    net::TcpListener,
    time::{timeout, Duration},
};

#[tokio::test]
async fn ping_pong() {
    // Create a pea2pea backed synthetic node and enable handshaking.
    let filter = MessageFilter::with_all_auto_reply();
    let mut synthetic_node =
        SyntheticNode::new(pea2pea::Node::new(None).await.unwrap(), true, filter);

    // Create a node and set the listener as an initial peer.
    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection(new_local_addr()))
        .start()
        .await;

    // Connect to the node and handshake.
    synthetic_node.connect(node.addr()).await.unwrap();

    // Send Ping.
    let ping_nonce = Nonce::default();
    synthetic_node
        .send_direct_message(node.addr(), Message::Ping(ping_nonce))
        .await
        .unwrap();

    // Recieve pong and verify the nonce matches.
    let message = synthetic_node.recv_message().await;
    assert_matches!(message, Message::Pong(pong_nonce) if pong_nonce == ping_nonce);

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
    //      Inv         (Invalid -- with mixed types)
    //      FilterLoad  (Obsolete)
    //      FilterAdd   (Obsolete)
    //      FilterClear (Obsolete)
    //
    // TBD: Inv         (Invalid -- with multiple advertised blocks)
    //      [todo: feedback from zcashd as to what the correct behaviour]
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
    //      Version:            passes
    //      Verack:             ignored
    //      Mixed Inv:          ignored
    //      Multi-Block Inv:    ignored
    //      FilterLoad:         Reject(Malformed) - needs investigation
    //      FilterAdd:          Reject(Malformed) - needs investigation
    //      FilterClear:        ignored
    //      FilterClear:        ignored
    //
    //  Zebra:
    //      All result in a terminated connection (no reject sent).

    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection(new_local_addr()))
        .start()
        .await;

    // generate a mixed Inventory hash set
    let genesis_block = Block::testnet_genesis();
    let mixed_inv = vec![genesis_block.inv_hash(), genesis_block.txs[0].inv_hash()];
    let multi_block_inv = vec![
        genesis_block.inv_hash(),
        genesis_block.inv_hash(),
        genesis_block.inv_hash(),
    ];

    // list of test messages and their expected Reject kind
    let cases = vec![
        (
            Message::Version(Version::new(node.addr(), new_local_addr())),
            CCode::Duplicate,
        ),
        (Message::Verack, CCode::Duplicate),
        (Message::Inv(Inv::new(mixed_inv)), CCode::Invalid),
        (Message::Inv(Inv::new(multi_block_inv)), CCode::Invalid),
        (Message::FilterLoad(FilterLoad::default()), CCode::Obsolete),
        (Message::FilterAdd(FilterAdd::default()), CCode::Obsolete),
        (Message::FilterClear, CCode::Obsolete),
    ];

    let filter = MessageFilter::with_all_auto_reply().enable_logging();

    for (test_message, expected_ccode) in cases {
        let mut stream = initiate_handshake(node.addr()).await.unwrap();

        test_message.write_to_stream(&mut stream).await.unwrap();
        // sending a ping which will let us see if `test_message` was ignored
        let nonce = Nonce::default();
        Message::Ping(nonce)
            .write_to_stream(&mut stream)
            .await
            .unwrap();

        // Expect a Reject(Invalid) message
        match filter.read_from_stream(&mut stream).await.unwrap() {
            Message::Reject(reject) if reject.ccode == expected_ccode => {}
            Message::Pong(n) if n == nonce => panic!("Message was ignored: {:?}", test_message),
            message => panic!(
                "Expected Reject({:?}), but got: {:?}",
                expected_ccode, message
            ),
        }
    }

    node.stop().await;
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

    let listener = TcpListener::bind(new_local_addr()).await.unwrap();

    // Create a node and set the listener as an initial peer.
    let mut node: Node = Default::default();
    node.initial_peers(vec![listener.local_addr().unwrap()])
        .start()
        .await;

    let mut stream = crate::helpers::respond_to_handshake(listener)
        .await
        .unwrap();

    let test_messages = vec![
        Message::Pong(Nonce::default()),
        Message::Headers(Headers::empty()),
        Message::Addr(Addr::empty()),
        Message::Block(Box::new(Block::testnet_genesis())),
        Message::NotFound(Inv::new(vec![Block::testnet_1().txs[0].inv_hash()])),
        Message::Tx(Block::testnet_2().txs[0].clone()),
    ];

    let filter = MessageFilter::with_all_auto_reply();

    for message in test_messages {
        message.write_to_stream(&mut stream).await.unwrap();

        let nonce = Nonce::default();
        Message::Ping(nonce)
            .write_to_stream(&mut stream)
            .await
            .unwrap();

        let pong = filter.read_from_stream(&mut stream).await.unwrap();
        assert_matches!(pong, Message::Pong(..));
    }

    node.stop().await;
}

#[tokio::test]
async fn basic_query_response() {
    // ZG-CONFORMANCE-010, node is seeded with data
    //
    // The node responds with the correct messages. Message correctness is naively verified through successful encoding/decoding.
    //
    // `Ping` expects `Pong`.
    // `GetAddr` expects `Addr`.
    // `Mempool` expects `Inv`.
    // `Getblocks` expects `Inv`.
    // `GetData(tx_hash)` expects `Tx`.
    // `GetData(block_hash)` expects `Blocks`.
    // `GetHeaders` expects `Headers`.
    //
    // The test currently fails for zcashd and zebra.
    //
    // Current behaviour:
    //
    //  zcashd: Ignores the following messages
    //              - GetAddr
    //              - MemPool
    //              - GetBlocks
    //
    //          GetData(tx) returns NotFound (which is correct),
    //          because we currently can't seed a mempool.
    //
    //  zebra: DoS `GetData` spam due to auto-response

    let mut node: Node = Default::default();
    node.initial_action(Action::SeedWithTestnetBlocks {
        socket_addr: new_local_addr(),
        block_count: 3,
    })
    .start()
    .await;

    let mut stream = initiate_handshake(node.addr()).await.unwrap();
    let filter = MessageFilter::with_all_auto_reply();
    let genesis_block = Block::testnet_genesis();

    Message::Ping(Nonce::default())
        .write_to_stream(&mut stream)
        .await
        .unwrap();
    let reply = filter.read_from_stream(&mut stream).await.unwrap();
    assert_matches!(reply, Message::Pong(..));

    Message::GetAddr.write_to_stream(&mut stream).await.unwrap();
    let reply = filter.read_from_stream(&mut stream).await.unwrap();
    assert_matches!(reply, Message::Addr(..));

    Message::MemPool.write_to_stream(&mut stream).await.unwrap();
    let reply = filter.read_from_stream(&mut stream).await.unwrap();
    assert_matches!(reply, Message::Inv(..));

    Message::GetBlocks(LocatorHashes::new(
        vec![genesis_block.double_sha256().unwrap()],
        Hash::zeroed(),
    ))
    .write_to_stream(&mut stream)
    .await
    .unwrap();
    let reply = filter.read_from_stream(&mut stream).await.unwrap();
    assert_matches!(reply, Message::Inv(..));

    Message::GetData(Inv::new(vec![genesis_block.txs[0].inv_hash()]))
        .write_to_stream(&mut stream)
        .await
        .unwrap();
    let reply = filter.read_from_stream(&mut stream).await.unwrap();
    assert_matches!(reply, Message::Tx(..));

    Message::GetData(Inv::new(vec![Block::testnet_2().inv_hash()]))
        .write_to_stream(&mut stream)
        .await
        .unwrap();
    let reply = filter.read_from_stream(&mut stream).await.unwrap();
    assert_matches!(reply, Message::Block(..));

    Message::GetHeaders(LocatorHashes::new(
        vec![genesis_block.double_sha256().unwrap()],
        Hash::zeroed(),
    ))
    .write_to_stream(&mut stream)
    .await
    .unwrap();
    let reply = filter.read_from_stream(&mut stream).await.unwrap();
    assert_matches!(reply, Message::Headers(..));

    node.stop().await;
}

#[tokio::test]
async fn basic_query_response_unseeded() {
    // ZG-CONFORMANCE-010, node is *not* seeded with data
    //
    // The node responds with the correct messages. Message correctness is naively verified through successful encoding/decoding.
    //
    // `GetData(tx_hash)` expects `NotFound`.
    // `GetData(block_hash)` expects `NotFound`.
    //
    // The test currently fails for zcashd and zebra
    //
    // Current behaviour:
    //
    //  zcashd: Ignores `GetData(block_hash)`
    //
    //  zebra: DDoS spam due to auto-response

    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection(new_local_addr()))
        .start()
        .await;

    let mut stream = initiate_handshake(node.addr()).await.unwrap();
    let filter = MessageFilter::with_all_auto_reply();
    let genesis_block = Block::testnet_genesis();

    Message::GetData(Inv::new(vec![genesis_block.txs[0].inv_hash()]))
        .write_to_stream(&mut stream)
        .await
        .unwrap();
    let reply = filter.read_from_stream(&mut stream).await.unwrap();
    assert_matches!(reply, Message::NotFound(..));

    Message::GetData(Inv::new(vec![Block::testnet_2().inv_hash()]))
        .write_to_stream(&mut stream)
        .await
        .unwrap();
    let reply = filter.read_from_stream(&mut stream).await.unwrap();
    assert_matches!(reply, Message::NotFound(..));

    node.stop().await;
}

#[tokio::test]
async fn disconnects_for_trivial_issues() {
    // ZG-CONFORMANCE-011
    //
    // The node disconnects for trivial (non-fuzz, non-malicious) cases.
    //
    // - `Ping` timeout (not tested due to 20minute zcashd timeout).
    // - `Pong` with wrong nonce.
    // - `GetData` with mixed types in inventory list.
    // - `Inv` with mixed types in inventory list.
    // - `Addr` with `NetworkAddr` with no timestamp.
    //
    // Note: Ping with timeout test case is not exercised as the zcashd timeout is
    //       set to 20 minutes, which is simply too long.
    //
    // Note: Addr test requires commenting out the relevant code in the encode
    //       function of NetworkAddr as we cannot encode without a timestamp.
    //
    // This test currently fails for zcashd and zebra.
    //
    // Current behaviour:
    //
    //  zcashd:
    //      GetData(mixed)  - responds to both
    //      Inv(mixed)      - ignores the message
    //      Addr            - Reject(Malformed), but no DC
    //
    //  zebra:
    //      Pong            - ignores the message

    // Create a node and main connection
    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection(new_local_addr()))
        .log_to_stdout(true)
        .start()
        .await;

    // NOTE: This test case is not exercised due to the extremely long timeout set
    //       by zcashd - 20minutes.
    //
    // Ping with timeout
    let filter = MessageFilter::with_all_auto_reply().with_ping_filter(Filter::Disabled);
    // let mut stream = initiate_handshake(node.addr()).await.unwrap();
    // match filter.read_from_stream(&mut stream).await.unwrap() {
    //     Message::Ping(_) => {
    //         match timeout(
    //             Duration::from_secs(120),
    //             filter.read_from_stream(&mut stream),
    //         )
    //         .await
    //         {
    //             Ok(Err(err)) if crate::helpers::is_termination_error(&err) => {}
    //             result => panic!("Expected termianted connection, but got {:?}", result),
    //         }
    //     }
    //     message => panic!("Unexpected message while waiting for Ping: {:?}", message),
    // }

    // Pong with bad nonce
    let mut stream = initiate_handshake(node.addr()).await.unwrap();
    match filter.read_from_stream(&mut stream).await.unwrap() {
        Message::Ping(_) => Message::Pong(Nonce::default())
            .write_to_stream(&mut stream)
            .await
            .unwrap(),

        message => panic!("Unexpected message while waiting for Ping: {:?}", message),
    }
    autorespond_and_expect_disconnect(&mut stream).await;

    // GetData with mixed inventory
    let genesis_block = Block::testnet_genesis();
    let mixed_inv = vec![genesis_block.inv_hash(), genesis_block.txs[0].inv_hash()];
    let mut stream = initiate_handshake(node.addr()).await.unwrap();
    Message::GetData(Inv::new(mixed_inv.clone()))
        .write_to_stream(&mut stream)
        .await
        .unwrap();
    autorespond_and_expect_disconnect(&mut stream).await;

    // Inv with mixed inventory (using non-genesis block since all node's "should" have genesis already,
    // which makes advertising it non-sensical)
    let mut stream = initiate_handshake(node.addr()).await.unwrap();
    let block_1 = Block::testnet_1();
    let mixed_inv = vec![block_1.inv_hash(), block_1.txs[0].inv_hash()];
    Message::Inv(Inv::new(mixed_inv))
        .write_to_stream(&mut stream)
        .await
        .unwrap();
    autorespond_and_expect_disconnect(&mut stream).await;

    // NOTE: Addr with missing timestamp cannot be run without modifying our code currently.
    //       NetworkAddr::encode will panic due to missing timestamp, so one needs to comment that
    //       section of code out before running this.
    //
    // let mut stream = initiate_handshake(node.addr()).await.unwrap();
    // let bad_addr = NetworkAddr::new(new_local_addr()).with_last_seen(None);
    // Message::Addr(Addr::new(vec![bad_addr]))
    //     .write_to_stream(&mut stream)
    //     .await
    //     .unwrap();
    // autorespond_and_expect_disconnect(&mut stream).await;

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
    // This test currently fails for zcashd; zebra fails (with a caveat).
    //
    // Current behaviour:
    //
    //  zcashd: Has different behaviour depending on connection direction.
    //          If we initiate the main connection it sends Ping, GetHeaders,
    //          but never GetAddr.
    //          If the node initiates then it does send GetAddr, but it never connects
    //          to the peers.
    //
    // zebra:   Fails, unless we keep responding on the main connection.
    //          If we do not keep responding then the peer connections take really long to establish,
    //          failing the test completely.
    //
    //          Related issues: https://github.com/ZcashFoundation/zebra/pull/2154
    //                          https://github.com/ZcashFoundation/zebra/issues/2163

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
    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection(new_local_addr()))
        .start()
        .await;

    // Start peer listeners which "pass" once they've accepted a connection, and
    // "fail" if the timeout expires. We expect this to happen quite quickly since
    // the node currently has very few active peers.
    let mut peer_handles = Vec::with_capacity(listeners.len());
    for listener in listeners {
        peer_handles.push(tokio::time::timeout(
            tokio::time::Duration::from_secs(20),
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
        .with_getaddr_filter(Filter::Disabled);

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

    // Wait for peer futures to complete
    for handle in peer_handles {
        handle.await.unwrap().unwrap();
    }

    node.stop().await;
}

#[tokio::test]
async fn correctly_lists_peers() {
    // ZG-CONFORMANCE-013
    //
    // The node responds to a `GetAddr` with a list of peers it’s connected to. This command
    // should only be sent once, and by the node initiating the connection.
    //
    // In addition, this test case exercises the known zebra bug: https://github.com/ZcashFoundation/zebra/pull/2120
    //
    // Test procedure
    //      1. Establish N peer listeners
    //      2. Start node which connects to these N peers
    //      3. Create i..M new connections which,
    //          a) Connect to the node
    //          b) Query GetAddr
    //          c) Receive Addr == N peer addresses
    //
    // This test currently fails for both zcashd and zebra.
    //
    // Current behaviour:
    //
    //  zcashd: Never responds. Logs indicate `Unknown command "getaddr" from peer=1` if we initiate
    //          the connection. If the node initiates the connection then the command is recoginized,
    //          but likely ignored (because only the initiating node is supposed to send it).
    //
    //  zebra:  Never responds: "zebrad::components::inbound: ignoring `Peers` request from remote peer during network setup"
    //
    //          Can be coaxed into responding by sending a non-empty Addr in
    //          response to node's GetAddr. This still fails as it includes previous inbound
    //          connections in its address book (as in the bug listed above).
    //

    // Establish N listener peer nodes
    const PEERS: usize = 5;
    let mut peers = Vec::with_capacity(PEERS);
    for _ in 0..PEERS {
        peers.push(TcpListener::bind(new_local_addr()).await.unwrap())
    }
    let mut peer_addrs = peers
        .iter()
        .map(|peer| peer.local_addr().unwrap())
        .collect::<Vec<_>>();
    peer_addrs.sort_unstable();
    let mut peer_handles = Vec::with_capacity(peers.len());
    for peer in peers {
        peer_handles.push(tokio::spawn(async move {
            peer.accept().await.unwrap();
        }));
    }

    // Start node with an initial set of peers to connect to
    let mut node: Node = Default::default();
    node.initial_action(Action::None)
        .initial_peers(peer_addrs.clone())
        .start()
        .await;

    // wait for all peers to get connected to
    for handle in peer_handles {
        handle.await.unwrap();
    }

    // List of inbound peer addresses (from the nodes perspective).
    // This is used to test that the node does not gossip these addresses.
    // (this exercises a known Zebra bug: https://github.com/ZcashFoundation/zebra/pull/2120)
    let mut inbound = Vec::new();

    // Connect to node and request GetAddr.
    // We perform multiple iterations in order to exercise the above Zebra bug.
    for i in 0..5 {
        let mut stream = initiate_handshake(node.addr()).await.unwrap();
        inbound.push(stream.local_addr().unwrap());

        Message::GetAddr.write_to_stream(&mut stream).await.unwrap();

        let filter = MessageFilter::with_all_auto_reply();

        match tokio::time::timeout(
            tokio::time::Duration::from_secs(5),
            filter.read_from_stream(&mut stream),
        )
        .await
        {
            Ok(Ok(Message::Addr(addresses))) => {
                let mut node_peers = addresses.iter().map(|net| net.addr).collect::<Vec<_>>();
                node_peers.sort_unstable();

                // Check that ephemeral connections were not gossiped. The next check would also catch this, but
                // this lets us add a more specific message.
                inbound.iter().for_each(|addr| {
                    assert!(
                        !node_peers.contains(addr),
                        "Iteration {}: Addr contains inbound peer address",
                        i
                    )
                });

                // Only the original peer listeners should be advertised
                assert_eq!(peer_addrs, node_peers, "Iteration {}:", i);
            }
            Ok(result) => panic!("Iteration {}: expected Ok(Addr), but got {:?}", i, result),
            Err(_timed_out) => panic!("Iteration {}: timeout waiting for `Addr`", i),
        }
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
    //  zcashd: Passes
    //
    //  zebra: does not support seeding as yet, and therefore cannot perform this test.
    //
    // Note: zcashd excludes the `stop_hash` from the range, whereas the spec states that it should be inclusive.
    //       We are taking current behaviour as correct.
    //
    // Note: zcashd ignores requests for the final block in the chain

    // Create a node with knowledge of the initial three testnet blocks
    let mut node: Node = Default::default();
    node.initial_action(Action::SeedWithTestnetBlocks {
        socket_addr: new_local_addr(),
        block_count: 3,
    })
    // .log_to_stdout(true)
    .start()
    .await;

    println!("Starting test!");

    let blocks = Block::initial_testnet_blocks();

    let mut stream = initiate_handshake(node.addr()).await.unwrap();
    let filter = MessageFilter::with_all_auto_reply().enable_logging();

    // Test unlimited range queries, where given the hash for block i we expect all
    // of its children as a reply. This does not apply for the last block in the chain,
    // so we skip it.
    //
    // i.e. Test that GetBlocks(i) -> Inv(i+1..)
    for (i, block) in blocks.iter().enumerate().take(2) {
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

    // Test that we get no response for the final block in the known-chain
    // (this is the behaviour exhibited by zcashd - a more well-formed response
    // might be sending an empty inventory instead).
    //
    // Test message is ignored by sending Ping and receiving Pong.
    Message::GetBlocks(LocatorHashes::new(
        vec![blocks.last().unwrap().double_sha256().unwrap()],
        Hash::zeroed(),
    ))
    .write_to_stream(&mut stream)
    .await
    .unwrap();
    let nonce = Nonce::default();
    Message::Ping(nonce)
        .write_to_stream(&mut stream)
        .await
        .unwrap();
    let reply = filter.read_from_stream(&mut stream).await.unwrap();
    assert_eq!(reply, Message::Pong(nonce));

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

    // Create a node with knowledge of the initial three testnet blocks
    let mut node: Node = Default::default();
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
        vec![hashes[0]],
        vec![hashes[1], hashes[0]],
        vec![hashes[2], hashes[1], hashes[0]],
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
        Message::GetHeaders(LocatorHashes::new(locator[start].clone(), hashes[stop]))
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

    // Create a node with knowledge of the initial three testnet blocks
    let mut node: Node = Default::default();
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
    let non_existant_inv = Inv::new(vec![non_existant]);
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
    mixed_blocks.insert(1, non_existant);
    mixed_blocks.push(non_existant);

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
    let mut node: Node = Default::default();
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
