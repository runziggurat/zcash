# Introduction

The purpose of this index is to provide an overview of the testing approaches to be implemented by Ziggurat. It is intended to evolve as the framework matures, leaving room for novel cases and extensions of existing cases, as called for by any protocol idiosyncrasies that may come to light during the development process.

Some test cases have been consolidated when similar behaviour is tested with differing messages. The final implementation of these cases will be subject to factors such as node setup and teardown details, test run time (and potentially runtime) constraints, readability and maintainability.

## Special Considerations

Some of these tests can be executed against the same node instance without resetting or clearing the cache to cut down on node setup and teardown times. Other tests are intended for use where a new peer is needed to clean the slate for a deterministic output.

Considering Zebra doesn't yet have an RPC implementation, tests making use of it could be feature-gated; some tests could have an extra and gated RPC assertion, for instance. Many of these tests treat the node as a black box and as such, don't necessitate any RPC calls to verify the behaviour under test. That being said, relying only on the connection to ascertain a node's reaction to a particular message should be, in time, reinforced with an RPC-powered assertion if available.

As for test data, [zcashd](https://github.com/zcash/zips/blob/master/zip-0243.rst#test-vector-1) and [zebra](https://github.com/ZcashFoundation/zebra/tree/main/zebra-test/src/vectors) vectors should provide ample coverage, including maximum-sized blocks with many transactions (for each pool type: transparent, sprout, sapling, orchard).

For load testing, "reasonable load" and "heavy load" will need to be defined.

## Usage

The tests can be run with `cargo test` once Ziggurat is properly configured and dependencies (node instance to be tested) are satisfied. See the [README](README.md) for details.

# Types of Tests

## Conformance

The conformance tests aim to verify the node adheres to the network protocol. In addition, they include some naive error cases with malicious and fuzzing cases consigned to the resistance tests. Most cases in this section will only require a socket standing in for the connected peer and a full node running in the background.

### Handshake

These tests verify the proper execution of a handshake between a node and a peer as well as some simple error cases.

### Post-handshake messages

These tests verify the node responds with the correct messages to requests and disconnects in certain trivial non-fuzz, non-malicious cases. These form the basic assumptions necessary for peering and syncing.

### Unsolicited post-handshake messages

These tests aim to evaluate the proper behaviour of a node when receiving unsolicited messages post-handshake.

### Simple peering

These tests evaluate the node's basic peering properties by building on ZG-CONFORMANCE-010 to verify the data included in the messages are in accordance with the peering status of the node.

### Simple sync

These tests evaluate the node's basic syncing properties for transactions and blocks by extending ZG-CONFORMANCE-010 to verify the data included in the message payloads are in accordance with the ranges provided by the peer.

## Performance

The performance tests aim to verify the node maintains a healthy throughput under pressure. This is principally done through simulating load with synthetic peers and evaluating the node's responsiveness. Synthetic peers will need to be able to simulate the behaviour of a full node by implementing handshaking, message sending and receiving.

### Load testing

These tests are intended to verify the node remains healthy under "reasonable load". Additionally these tests will be pushed to the extreme for resistance testing with heavier loads.

### Heavy load testing

These tests are meant to explore the impact of malicious network use against a node.

The amount of load and its frequency could be modulated to provide a comprehensive verification of the node's behaviour under different conditions (including synchronized requests from different peers and other worst case scenarios).

## Resistance

The resistance tests are designed for the early detection and avoidance of weaknesses exploitable through malicious behaviour. They attempt to probe boundary conditions with comprehensive fuzz testing and extreme load testing. The nature of the peers in these cases will depend on how accurately they needs to simulate node behaviour. It will likely be a mixture of simple sockets for the simple cases and peers used in the performance tests for the more advanced.

### Fuzz testing

The fuzz tests aim to buttress the message conformance tests with extra verification of expected node behaviour when receiving corrupted or broken messages. Our approach is targeting these specific areas and we anticipate broadening these test scenarios as necessary:

- Messages with any length and any content (random bytes).
- Messages with plausible lengths, e.g. 24 bytes for header and within the expected range for the body.
- Metadata-compliant messages, e.g. correct header, random body.
- Slightly corrupted but otherwise valid messages, e.g. N% of body replaced with random bytes.
- Messages with an incorrect checksum.
- Messages with differing announced and actual lengths.

# Test Index

## Conformance

### ZG-CONFORMANCE-001

    The node correctly performs a handshake from the responder side.

    1. Connect to the node under test.
    2. Send the initial `Version` and complete handshake.
    3. Assert the node’s peer count has increased to 1 and/or the synthetic node is an established peer (rpc: `getconnectioncount` and/or `getpeerinfo`).

### ZG-CONFORMANCE-002

    The node correctly performs a handshake from the initiator side.

    1. The node under test initiates a connection (rpc: `addnode`).
    2. Receive the initial `Version` and complete handshake.
    3. Assert the node’s peer count has increased to 1 and/or the synthetic node is an established peer.

### ZG-CONFORMANCE-003

    The node ignores non-`Version` messages before the handshake has been performed.

    1. Connect to the node under test.
    2. Send non-`Version` messages.
    3. Assert the node ignored the message by completing the handshake.

### ZG-CONFORMANCE-004

    The node ignores non-`Version` messages in response to the initial `Version` it sent.

    1. The node under test initiates a connection (rpc: `addnode`).
    2. Respond to `Version` with non-`Version` messages.
    3. Assert the node ignored the message by completing the handshake.

### ZG-CONFORMANCE-005

    The node ignores non-`Verack` message as a response to initial `Verack` it sent.

    1. The node under test initiates a connection (rpc: `addnode`).
    2. Respond to `Version`, expect `Verack`.
    3. Respond to `Verack` with non-`Verack` messages.
    4. Assert the node ignored the message by completing the handshake.

### ZG-CONFORMANCE-006

    The node rejects connections reusing its `nonce` (usually indicative of self-connection).

    1. The node under test initiates a connection (rpc: `addnode`).
    2. Respond to received `Version` with the node’s nonce.
    3. Assert the node closed the connection.

### ZG-CONFORMANCE-007

    The node rejects connections with obsolete node versions.

    1. Initiator or responder handshake.
    2. Peer sends `Version` with an obsolete version.
    3. Assert the node rejected the connection.

### ZG-CONFORMANCE-008

    The node rejects handshake and bloom filter messages post-handshake.
    Zcash nodes used to support this by default, without advertising this
    bit, but no longer do as of protocol version 170004 `(= NO_BLOOM_VERSION)`.

    1. Establish handshaken node and peer.
    2. Send unsolicited message to be rejected.
    3. Assert the node rejected the unsolicited message and dropped the connection.

    Messages to be tested: `Version`, `Verack`, `FilterLoad`, `FilterAdd`, `FilterClear`, `Inv` with multiple advertised blocks (multiple transactions or single block payloads don’t get rejected).

### ZG-CONFORMANCE-009

    The node ignore certain unsolicited messages but doesn’t disconnect.

    1. Establish handshaken node and peer.
    2. Send an unsolicited message to be ignored.
    3. Assert the node ignored the unsolicited message and didn’t drop the connection (not sure how yet, perhaps by sending a ping and verifying that works
    as intended).

    Messages to be tested: `Reject`, `NotFound`, `Pong`, `Tx`, `Block`, `Header`, `Addr`.

### ZG-CONFORMANCE-010

    The node responds with the correct messages. Message correctness is naively verified through successful encoding/decoding.

    1. Establish handshaken node and peer.
    2. Send message.
    3. Receive response and assert it is correct.

    Messages to be tested:

    - `Ping` expects `Pong`.
    - `GetAddr` expects `Addr`.
    - `Mempool` expects `Inv`.
    - `Getblocks` expects `Inv`.
    - `GetData` expects `Tx`.
    - `GetData` expects `Blocks`.
    - `GetHeaders` expects `Headers`.

### ZG-CONFORMANCE-011

    The node disconnects for trivial (non-fuzz, non-malicious) cases.

    - `Ping` timeout.
    - `Pong` with wrong nonce.
    - `GetData` with mixed types in inventory list.
    - `Inv` with mixed types in inventory list.
    - `Addr` with `NetworkAddr` with no timestamp.

### ZG-CONFORMANCE-012
    The node crawls the network for new peers and eagerly connects.

    1. Node sends a `GetAddr`.
    2. Peer responds with `Addr` containing a list of peers to connect to.
    3. Assert peers get a connection request from the node.

### ZG-CONFORMANCE-013
    The node responds to a `GetAddr` with a list of peers it’s connected to.

    1. Establish handshaken node with multiple peers.
    2. Peer sends `GetAddr`.
    3. Node responds with `Addr`.
    4. Assert the node's connected peers were included.

### ZG-CONFORMANCE-014

    The node responds to `Mempool` requests with a list of transactions in its memory pool.

    1. Establish handshaken node and peer.
    2. Peer sends `Mempool` request.
    3. Expect an `Inv` response containing all the transaction hashes in the node's memory pool.

### ZG-CONFORMANCE-015

    The node responds to `GetBlocks` requests with a list of blocks based on the provided range.

    1. Establish handshaken node and peer.
    2. Peer sends `GetBlocks` request (different ranges could be tested).
    3. Expect an `Inv` response containing the adequate data based on the requested range.

### ZG-CONFORMANCE-016

    The node responds to `GetHeaders` request with a list of block headers based on the provided range.

    1. Establish handshaken node and peer.
    2. Peer sends `GetHeaders` request (different ranges could be tested).
    3. Expect a `Headers` response containing the adequate data based on the requested range.

### ZG-CONFORMANCE-017

    The node responds to `GetData` requests with the appropriate transaction or block as requested by the peer.

    1. Establish handshaken node and peer.
    2. Peer sends `GetData` asking for transactions or blocks.
    3. Expect one of the following responses to be appropriate: `Tx`, `Block` and `NotFound`.

## Performance

### ZG-PERFORMANCE-001

    The node behaves as expected under load from other peers.

    1. Establish a node and synthetic peers.
    2. Begin simulation.
    3. Introspect node health and responsiveness through peers (latency, throughput). This could be done using `Ping`/`Pong` messages. In extreme cases, node crash (should it occur) could be detectable through `tokio::process::command` (`SIGKILL`, etc...).

### ZG-PERFORMANCE-002

    The node sheds or rejects connections when necessary.

    1. Establish a node.
    2. Connect and handshake synthetic peers until peer threshold is reached.
    3. Expect connections to be dropped and/or the node's peer count to diminish.

## Resistance

### ZG-RESISTANCE-001

    The node rejects various random bytes pre-handshake

    1. Connect to node.
    2. Send random bytes.
    3. Assert connection rejected.

### ZG-RESISTANCE-002

    The node rejects various random bytes during handshake responder side.

    1. Connect to node and initiate handshake.
    2. Send and receive `Version`.
    3. Respond with random bytes in place of `Verack`.
    4. Assert connection rejected.

### ZG-RESISTANCE-003

    The node rejects various random bytes during handshake initiator side (`Version`).

    1. Respond to Version with random bytes in place of `Version`.
    2. Assert connection rejected.

### ZG-RESISTANCE-004

    The node rejects various random bytes during handshake initiator side (`Verack`).

    1. Respond to `Version` with valid `Version`.
    2. Receive `Verack` and respond with random bytes.
    3. Assert connection rejected.

### ZG-RESISTANCE-005

    The node rejects various random bytes post-handshake.

    1. Establish handshaken node and peer.
    2. Send random bytes.
    3. Assert connection rejected.

### ZG-RESISTANCE-006

    This is the sister test to ZG-PERFORMANCE-001 with higher connection numbers. As in ZG-PERFORMANCE-002, we also expect to see load shedding and connection rejections when necessary.

    Variations on this test include:

    - Spamming messages (including fuzzed).
    - Spamming connections and/or reconnections.
