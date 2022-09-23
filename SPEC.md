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

Tests are grouped into the following categories: conformance, performance, and resistance. Each test is named after the category it belongs to, in addition to what's being tested. For example, `c001_handshake_when_node_receives_connection` is the first conformance test and tests the handshake behavior on the receiving end. The full naming convention is: `id_part_t(subtest_no)_(message type)_(extra_test_desc)`.

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

The test index makes use of symbolic language in describing connection and message sending directions. As a convention, Ziggurat test nodes are to the left of the connection/message arrows, and Zebra or Zcashd instances are to the right: `A -> B` and `A <- B`. In this way, `->` signifies "Ziggurat connects to Zcashd or Zebra" and `<-` signifies the opposite. Furthermore, `-> version` signifies "Ziggurat sends a `Version` message to Zcashd or Zebra" and `<- version` signifies the opposite. Lastly, `<>` signifies a completed handshake, in either direction.

## Conformance

### ZG-CONFORMANCE-001

    The node correctly performs a handshake from the responder side.

    ->
    -> version
    <- version
    -> verack
    <- verack

    Assert: the node’s peer count has increased to 1 and/or the synthetic node is an established peer (rpc: `getconnectioncount` and/or `getpeerinfo`).

### ZG-CONFORMANCE-002

    The node correctly performs a handshake from the initiator side.

    <-
    <- version
    -> version
    <- verack
    -> verack

    Assert: the node’s peer count has increased to 1 and/or the synthetic node is an established peer.

### ZG-CONFORMANCE-003

    The node ignores non-`Version` messages before the handshake has been performed.

    Let M be a `non-Version` message.

    <-
    -> M

    Assert: the message was ignored (by completing the handshake).

    -> version
    <- version
    -> verack
    <- verack

### ZG-CONFORMANCE-004

    The node ignores non-`Version` messages in response to the initial `Version` it sent.

    Let M be a `non-Version` message.

    <-
    <- version
    -> non-version

    Assert: the message was ignored (by completing the handshake).

    -> version
    <- verack
    -> verack

### ZG-CONFORMANCE-005

    The node ignores non-`Verack` message as a response to initial `Version` it sent.

    Let M be a non-Verack message.

    ->
    -> version
    <- version
    -> M

    Assert: the message was ignored (by completing the handshake).

    -> verack
    <- verack

### ZG-CONFORMANCE-006

    The node ignores non-`Verack` message as a response to initial `Verack` it sent.

    <-
    <- version
    -> version
    <- verack
    -> non-verack

    Assert: the node ignored the message (by completing the handshake).

    -> verack

### ZG-CONFORMANCE-007

    The node rejects connections reusing its `nonce` (usually indicative of self-connection).

    Let N be the node's nonce.

    <-
    <- version(N)
    -> version(N)

    Assert: the node closed the connection.

### ZG-CONFORMANCE-008

    The node rejects connections with obsolete node versions.

    Let O be an obsolete protocol version number.

    ->
    -> version(O)

    or

    <-
    <- version
    -> version(O)

    Assert: the node rejected the connection.

### ZG-CONFORMANCE-009

    The node rejects handshake and bloom filter messages post-handshake.

    Zcash nodes used to support this by default, without advertising this bit, but no longer do as of protocol version 170004 `(= NO_BLOOM_VERSION)`.

    Let M be a `Version`, `Verack`, `FilterLoad`, `FilterAdd`, `FilterClear` or `Inv` message with multiple advertised blocks (multiple transactions or single block payloads don’t get rejected).

    <>
    -> M

    Assert: the node rejected the unsolicited message and dropped the connection.

### ZG-CONFORMANCE-010

    The node ignore certain unsolicited messages but doesn’t disconnect.

    Let M be a `Reject`, `NotFound`, `Pong`, `Tx`, `Block`, `Header` or `Addr` message.

    <>
    -> M

    Assert: the node ignored the unsolicited message and didn’t drop the connection.

### ZG-CONFORMANCE-011

    The node responds with the correct messages. Message correctness is naively verified through successful encoding/decoding.

    Let Q, R be the query and response pairs to be tested:

    - `Ping` expects `Pong`.
    - `GetAddr` expects `Addr`.
    - `Mempool` expects `Inv`.
    - `Getblocks` expects `Inv`.
    - `GetData` expects `Tx`.
    - `GetData` expects `Blocks`.
    - `GetHeaders` expects `Headers`.

    <>
    -> Q
    <- R

    Assert: the appropriate response is sent.

### ZG-CONFORMANCE-012

    The node disconnects for trivial (non-fuzz, non-malicious) cases.

    - `Pong` with wrong nonce.
    - `GetData` with mixed types in inventory list.
    - `Inv` with mixed types in inventory list.
    - `Addr` with `NetworkAddr` with no timestamp.

### ZG-CONFORMANCE-013

    The node crawls the network for new peers and eagerly connects.

    <>
    <- getaddr
    -> addr(addrs)

    Assert: peers (addrs) get a connection request from the node.

### ZG-CONFORMANCE-014

    The node responds to a `GetAddr` with a list of peers it’s connected to.

    <> (with N synthetic nodes)
    -> getaddr
    <- addr(N addrs)

    Assert: the node's connected peers were sent in the payload.

### ZG-CONFORMANCE-015

    The node responds to `Mempool` requests with a list of transactions in its memory pool.

    Let T be the tx hashes seeded in the node's memory pool.

    <>
    -> mempool
    <- inv(T)

    Assert: the `Inv` response contains all the tx hashes in the node's memory pool.

### ZG-CONFORMANCE-016

    The node responds to `GetBlocks` requests with a list of blocks based on the provided range.

    Let R be a block range, D the adequate data corresponding to R.

    <>
    -> getblocks(R)
    <- inv(D)

    Assert: the `Inv` response contains all the block hashes in the supplied range (if the node has them).

### ZG-CONFORMANCE-017

    The node responds to `GetHeaders` request with a list of block headers based on the provided range.

    Let R be a header range, D the adequate data corresponding to R.

    <>
    -> getheaders(R)
    <- headers(D)

    Assert: the `Headers` response contains the headers in the requested range (if the node has them).

### ZG-CONFORMANCE-018

    The node responds to `GetData` requests with the appropriate transaction or block as requested by the peer.

    Let Q be a query for transactions or blocks, R a `Tx`, `Block` or `NotFound` message as appropriate based on Q.

    <>
    -> getdata(Q)
    <- R

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

Important note: The following tests generelly assert that a connection from an illicit node gets rejected. However, ZG-RESISTANCE-00* part-5 (`bad_checksum`) will instead assert that the connection **does not** get rejected, due to that being the canonical `zcashd` behavior.

### ZG-RESISTANCE-001

    The node rejects various random bytes pre-handshake

    -> random bytes

    Assert: the connection is rejected.

### ZG-RESISTANCE-002

    The node rejects various random bytes during handshake responder side.

    ->
    -> version
    <- version
    -> random bytes

    Assert: the connection is rejected.

### ZG-RESISTANCE-003

    The node rejects various random bytes during handshake initiator side (`Version`).

    <-
    <- version
    -> random bytes

    Assert: the connection is rejected.

### ZG-RESISTANCE-004

    The node rejects various random bytes during handshake initiator side (`Verack`).

    <-
    <- version
    -> version
    <- verack
    -> random bytes

    Assert: the connection is rejected.

### ZG-RESISTANCE-005

    The node rejects various random bytes post-handshake.

    <>
    -> random bytes

    Assert: the connection is rejected.

### ZG-RESISTANCE-006

    This is the sister test to ZG-PERFORMANCE-001 with higher connection numbers. As in ZG-PERFORMANCE-002, we also expect to see load shedding and connection rejections when necessary.

    Variations on this test include:

    - Spamming messages (including fuzzed).
    - Spamming connections and/or reconnections.
