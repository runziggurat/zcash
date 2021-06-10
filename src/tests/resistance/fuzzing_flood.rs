use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Cell, CellAlignment, Table};
use histogram::Histogram;
use rand::{prelude::SliceRandom, Rng};
use rand_chacha::ChaCha8Rng;
use std::{convert::TryFrom, io::Write, net::SocketAddr};
use tokio::{sync::mpsc::Sender, time::Duration};

use crate::{
    helpers::{initiate_handshake, is_rejection_error, is_termination_error},
    protocol::{
        message::{constants::MAGIC, filter::MessageFilter, Message, MessageHeader},
        payload::{
            block::{Block, Headers, LocatorHashes},
            codec::Codec,
            Hash, Inv, Nonce,
        },
    },
    setup::{
        config::new_local_addr,
        node::{Action, Node},
    },
    tests::resistance::{
        default_fuzz_messages,
        fuzzing_corrupted_messages::slightly_corrupted_messages,
        fuzzing_incorrect_checksum::encode_messages_and_corrupt_checksum,
        fuzzing_incorrect_length::encode_messages_and_corrupt_body_length_field,
        fuzzing_random_bytes::{metadata_compliant_random_bytes, random_bytes},
        fuzzing_zeroes::zeroes,
        seeded_rng, COMMANDS_WITH_PAYLOADS,
    },
};

enum Event {
    HandshakeEstablished(Duration),
    HandshakeRejected,
    HandshakeError(std::io::Error),
    ValidReply(Duration),
    BadReply(Box<Message>, Box<Message>),
    WriteError(std::io::Error),
    ReadError(std::io::Error),
    Dropped,
    Terminated,
    IgnoredCorrupt(Vec<u8>),
    RejectedCorrupt,
    RepliedToCorrupt(Box<Message>),
    Complete,
}

#[derive(Default, Debug)]
struct Stats {
    handshake_accepted: u16,
    current_connections: u16,
    max_active_connections: u16,
    handshake_latencies: Vec<Duration>,
    handshake_rejected: u16,
    peers_dropped: u16,
    reply_latencies: Vec<Duration>,
    corrupt_terminated: u16,
    corrupt_rejected: u16,
    reply_instead_of_termination: u16,
    corrupt_ignored: u16,

    reply_errors: u16,
    read_errors: u16,
    write_errors: u16,
    handshake_errors: u16,
    dangling: u16,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn rising_fuzz() {
    // ZG-RESISTANCE-006
    //
    // Simulate high load situations, while also sending corrupt messages.
    //
    // We simulate active peers which send 100 requests (and validate the replies), and finally send a corrupt message
    // which ends the connection. We measure the latency of the replies and the type of response to the corrupt message.
    //
    //  *NOTE* run with `cargo test --release tests::resistance::fuzzing_flood::rising_fuzz -- --nocapture`
    //
    // Currently only works for zcashd as zebra does not support block seeding.
    //
    // ZCashd: Terminates most of the time, but also rejects and ignores some of the messages. In some instances the connection hangs,
    //         all instances investigated were as a result of a corrupt payload length leading to the node waiting for more data.
    //         This is likely the desired and correct behaviour.
    //
    // Stats
    // ╭───────┬──────────┬─────────────┬───────────┬───────────┬───────────┬────────────┬────────────┬──────────┬─────────┬─────────┬───────────┬─────────────┬────────────┬──────────╮
    // │ Peers ┆ Requests ┆ Peak Active ┆ Handshake ┆ Handshake ┆ Handshake ┆ Connection ┆   Corrupt  ┆  Corrupt ┆ Corrupt ┆ Corrupt ┆ IO Errors ┆ Bad replies ┆    Hung    ┆ Time (s) │
    // │       ┆          ┆    Peers    ┆  Accepted ┆  Rejected ┆   Errors  ┆   Dropped  ┆ Terminated ┆ Rejected ┆ Ignored ┆ Replied ┆           ┆             ┆ Connection ┆          │
    // ╞═══════╪══════════╪═════════════╪═══════════╪═══════════╪═══════════╪════════════╪════════════╪══════════╪═════════╪═════════╪═══════════╪═════════════╪════════════╪══════════╡
    // │     1 ┆      100 ┆           1 ┆         1 ┆         0 ┆         0 ┆          1 ┆          0 ┆        0 ┆       0 ┆       0 ┆         0 ┆           0 ┆          0 ┆     0.07 │
    // ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌─┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┤
    // │    10 ┆      100 ┆          10 ┆        10 ┆         0 ┆         0 ┆          0 ┆          8 ┆        0 ┆       0 ┆       0 ┆         0 ┆           0 ┆          2 ┆    11.15 │
    // ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌─┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┤
    // │    20 ┆      100 ┆          19 ┆        20 ┆         0 ┆         0 ┆          1 ┆         16 ┆        0 ┆       1 ┆       0 ┆         0 ┆           0 ┆          2 ┆    22.20 │
    // ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌─┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┤
    // │    30 ┆      100 ┆          30 ┆        30 ┆         0 ┆         0 ┆          2 ┆         21 ┆        5 ┆       2 ┆       0 ┆         0 ┆           0 ┆          0 ┆    23.06 │
    // ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌─┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┤
    // │    40 ┆      100 ┆          40 ┆        40 ┆         0 ┆         0 ┆          0 ┆         28 ┆        3 ┆       6 ┆       0 ┆         0 ┆           0 ┆          3 ┆    33.93 │
    // ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌─┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┤
    // │    50 ┆      100 ┆          49 ┆        50 ┆         0 ┆         0 ┆          0 ┆         38 ┆        3 ┆       5 ┆       0 ┆         0 ┆           0 ┆          4 ┆    44.85 │
    // ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌─┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┤
    // │    60 ┆      100 ┆          57 ┆        60 ┆         0 ┆         0 ┆          2 ┆         43 ┆        2 ┆       6 ┆       0 ┆         0 ┆           0 ┆          7 ┆    55.77 │
    // ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌─┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┤
    // │    70 ┆      100 ┆          68 ┆        70 ┆         0 ┆         0 ┆          1 ┆         44 ┆        8 ┆      12 ┆       0 ┆         0 ┆           0 ┆          5 ┆    66.77 │
    // ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌─┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┤
    // │    80 ┆      100 ┆          80 ┆        80 ┆         0 ┆         0 ┆          3 ┆         67 ┆        2 ┆       7 ┆       0 ┆         0 ┆           0 ┆          1 ┆    78.04 │
    // ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌─┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┤
    // │    90 ┆      100 ┆          89 ┆        90 ┆         0 ┆         0 ┆          0 ┆         64 ┆       10 ┆      12 ┆       0 ┆         0 ┆           0 ┆          4 ┆    89.40 │
    // ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌─┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┤
    // │   100 ┆      100 ┆          96 ┆       100 ┆         0 ┆         0 ┆          1 ┆         80 ┆        2 ┆      11 ┆       0 ┆         0 ┆           0 ┆          6 ┆   100.58 │
    // ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌─┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┤
    // │   200 ┆      100 ┆         195 ┆       200 ┆         0 ┆         0 ┆          4 ┆        143 ┆       11 ┆      25 ┆       0 ┆         0 ┆           0 ┆         17 ┆   112.82 │
    // ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌─┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┤
    // │   300 ┆      100 ┆         296 ┆       300 ┆         0 ┆         0 ┆          3 ┆        236 ┆       13 ┆      31 ┆       0 ┆         0 ┆           0 ┆         17 ┆   125.73 │
    // ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌─┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┤
    // │   500 ┆      100 ┆         471 ┆       500 ┆         0 ┆         0 ┆          3 ┆        368 ┆       26 ┆      73 ┆       0 ┆         0 ┆           0 ┆         30 ┆   140.38 │
    // ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌─┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┤
    // │   750 ┆      100 ┆         702 ┆       750 ┆         0 ┆         0 ┆          8 ┆        561 ┆       34 ┆     106 ┆       0 ┆         0 ┆           0 ┆         41 ┆   156.88 │
    // ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌─┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┤
    // │   800 ┆      100 ┆         687 ┆       800 ┆         0 ┆         0 ┆         11 ┆        591 ┆       54 ┆      95 ┆       0 ┆         0 ┆           0 ┆         49 ┆   173.76 │
    // ╰───────┴──────────┴─────────────┴───────────┴───────────┴───────────┴────────────┴────────────┴──────────┴─────────┴─────────┴───────────┴─────────────┴────────────┴──────────╯

    // Request latencies
    // ╭───────┬──────────┬──────────┬──────────┬─────────────┬──────────┬──────────┬──────────┬──────────┬──────────┬───────────╮
    // │ Peers ┆ Requests ┆ Min (ms) ┆ Max (ms) ┆ stddev (ms) ┆ 10% (ms) ┆ 50% (ms) ┆ 75% (ms) ┆ 90% (ms) ┆ 99% (ms) ┆ Request/s │
    // ╞═══════╪══════════╪══════════╪══════════╪═════════════╪══════════╪══════════╪══════════╪══════════╪══════════╪═══════════╡
    // │     1 ┆      100 ┆        0 ┆        0 ┆           0 ┆        0 ┆        0 ┆        0 ┆        0 ┆        0 ┆    801.47 │
    // ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
    // │    10 ┆      100 ┆        0 ┆       52 ┆          20 ┆        0 ┆        1 ┆        1 ┆       51 ┆       51 ┆     50.51 │
    // ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
    // │    20 ┆      100 ┆        0 ┆       53 ┆          16 ┆        0 ┆        1 ┆        2 ┆       39 ┆       52 ┆     41.98 │
    // ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
    // │    30 ┆      100 ┆        0 ┆       61 ┆          17 ┆        1 ┆        3 ┆        4 ┆       51 ┆       54 ┆     68.47 │
    // ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
    // │    40 ┆      100 ┆        0 ┆       59 ┆          14 ┆        1 ┆        3 ┆        5 ┆       22 ┆       54 ┆     56.36 │
    // ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
    // │    50 ┆      100 ┆        0 ┆      164 ┆          20 ┆        2 ┆        5 ┆        6 ┆       12 ┆      143 ┆     61.70 │
    // ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
    // │    60 ┆      100 ┆        0 ┆       68 ┆           8 ┆        3 ┆        5 ┆        7 ┆        9 ┆       50 ┆     54.38 │
    // ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
    // │    70 ┆      100 ┆        0 ┆       72 ┆          11 ┆        2 ┆        6 ┆        8 ┆       12 ┆       54 ┆     54.02 │
    // ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
    // │    80 ┆      100 ┆        0 ┆       97 ┆          17 ┆        2 ┆        7 ┆       10 ┆       49 ┆       61 ┆     51.10 │
    // ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
    // │    90 ┆      100 ┆        0 ┆       95 ┆          16 ┆        3 ┆        9 ┆       15 ┆       24 ┆       66 ┆     52.18 │
    // ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
    // │   100 ┆      100 ┆        0 ┆      103 ┆          14 ┆        3 ┆        9 ┆       12 ┆       23 ┆       63 ┆     48.84 │
    // ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
    // │   200 ┆      100 ┆        0 ┆      110 ┆          16 ┆        8 ┆       21 ┆       28 ┆       50 ┆       71 ┆     88.18 │
    // ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
    // │   300 ┆      100 ┆        0 ┆      121 ┆          17 ┆       11 ┆       27 ┆       37 ┆       53 ┆       76 ┆    112.01 │
    // ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
    // │   500 ┆      100 ┆        0 ┆      183 ┆          19 ┆       20 ┆       49 ┆       58 ┆       64 ┆       98 ┆    177.93 │
    // ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
    // │   750 ┆      100 ┆        0 ┆      234 ┆          26 ┆       30 ┆       69 ┆       82 ┆       91 ┆      168 ┆    233.46 │
    // ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
    // │   800 ┆      100 ┆        0 ┆      205 ┆          26 ┆       30 ┆       70 ┆       83 ┆       92 ┆      149 ┆    226.12 │
    // ╰───────┴──────────┴──────────┴──────────┴─────────────┴──────────┴──────────┴──────────┴──────────┴──────────┴───────────╯

    let mut request_table = Table::new();
    request_table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("Peers").set_alignment(CellAlignment::Center),
            Cell::new("Requests").set_alignment(CellAlignment::Center),
            Cell::new("Min (ms)").set_alignment(CellAlignment::Center),
            Cell::new("Max (ms)").set_alignment(CellAlignment::Center),
            Cell::new("stddev (ms)").set_alignment(CellAlignment::Center),
            Cell::new("10% (ms)").set_alignment(CellAlignment::Center),
            Cell::new("50% (ms)").set_alignment(CellAlignment::Center),
            Cell::new("75% (ms)").set_alignment(CellAlignment::Center),
            Cell::new("90% (ms)").set_alignment(CellAlignment::Center),
            Cell::new("99% (ms)").set_alignment(CellAlignment::Center),
            Cell::new("Request/s").set_alignment(CellAlignment::Center),
        ]);

    let mut stats_table = Table::new();
    stats_table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("Peers").set_alignment(CellAlignment::Center),
            Cell::new("Requests").set_alignment(CellAlignment::Center),
            Cell::new("Peak Active\nPeers").set_alignment(CellAlignment::Center),
            Cell::new("Handshake\nAccepted").set_alignment(CellAlignment::Center),
            Cell::new("Handshake\nRejected").set_alignment(CellAlignment::Center),
            Cell::new("Handshake\nErrors").set_alignment(CellAlignment::Center),
            Cell::new("Connection\nDropped").set_alignment(CellAlignment::Center),
            Cell::new("Corrupt\nTerminated").set_alignment(CellAlignment::Center),
            Cell::new("Corrupt\nRejected").set_alignment(CellAlignment::Center),
            Cell::new("Corrupt\nIgnored").set_alignment(CellAlignment::Center),
            Cell::new("Corrupt\nReplied").set_alignment(CellAlignment::Center),
            Cell::new("IO Errors").set_alignment(CellAlignment::Center),
            Cell::new("Bad replies").set_alignment(CellAlignment::Center),
            Cell::new("Hung\nConnection").set_alignment(CellAlignment::Center),
            Cell::new("Time (s)").set_alignment(CellAlignment::Center),
        ]);

    // Create a pool of valid and invalid message types
    const MAX_VALID_MESSAGES: usize = 100;
    let peer_counts = vec![
        1, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 200, 300, 500, 750, 800,
    ];

    let mut rng = seeded_rng();
    let valid_pool = valid_queries_responses();

    // Start node with arbitrarily higher max peer count than what we
    // need for the test. Note that zcashd node appears to reserver 8
    // slots (hence the +10).
    let mut node: Node = Default::default();
    node.initial_action(Action::SeedWithTestnetBlocks(3))
        .max_peers(peer_counts.iter().max().unwrap() * 2 + 10)
        .start()
        .await;

    let node_addr = node.addr();

    let iteration_timer = tokio::time::Instant::now();

    // Iterate over peer counts
    // (note: rng usage should stay in a single thread in order for it be somewhat repeatable - we can't account
    // for relative timings and state transitions in the node).
    for peers in peer_counts {
        // Generate the broken fuzz messages for this peer set (one per peer, since this should break the connection)
        let mut corrupt_messages = generate_corrupt_messages(&mut rng, peers);

        // start event manager
        let (event_tx, event_rx) = tokio::sync::mpsc::channel::<Event>(peers * 50);
        let event_manager = tokio::spawn(manage_events(event_rx, peers));

        // Start the N peers for this iteration
        let mut peer_handles = Vec::with_capacity(peers);
        let mut peer_exits = Vec::with_capacity(peers);
        for _ in 0..peers {
            let (exit_tx, exit_rx) = tokio::sync::oneshot::channel::<()>();
            peer_exits.push(exit_tx);

            // generate the valid message set for this peer
            let valid = (0..rng.gen_range(0..MAX_VALID_MESSAGES))
                .map(|_| valid_pool.choose(&mut rng).unwrap().clone())
                .collect::<Vec<_>>();
            // grab the broken message for this peer
            let corrupt = corrupt_messages.pop().unwrap();

            let peer_tx = event_tx.clone();

            // start and wait for peer simulation to complete, or cancel if exit signal is received
            peer_handles.push(tokio::spawn(async move {
                tokio::select! {
                    _ = exit_rx => {},
                    _ = simulate_peer(node_addr, peer_tx, valid, corrupt) => {},
                }
            }));
        }

        // wait for event manager to complete
        let stats = event_manager.await.unwrap();

        let iteration_time = iteration_timer.elapsed();

        // Send stop signal to peer nodes. We ignore the possible error
        // result as this will occur with peers that have already exited.
        for stop in peer_exits {
            let _ = stop.send(());
        }

        // Wait for peers to complete
        for handle in peer_handles {
            handle.await.unwrap();
        }

        // update stats tables
        stats_table.add_row(vec![
            Cell::new(peers).set_alignment(CellAlignment::Right),
            Cell::new(MAX_VALID_MESSAGES).set_alignment(CellAlignment::Right),
            Cell::new(stats.max_active_connections).set_alignment(CellAlignment::Right),
            Cell::new(stats.handshake_accepted).set_alignment(CellAlignment::Right),
            Cell::new(stats.handshake_rejected).set_alignment(CellAlignment::Right),
            Cell::new(stats.handshake_errors).set_alignment(CellAlignment::Right),
            Cell::new(stats.peers_dropped).set_alignment(CellAlignment::Right),
            Cell::new(stats.corrupt_terminated).set_alignment(CellAlignment::Right),
            Cell::new(stats.corrupt_rejected).set_alignment(CellAlignment::Right),
            Cell::new(stats.corrupt_ignored).set_alignment(CellAlignment::Right),
            Cell::new(stats.reply_instead_of_termination).set_alignment(CellAlignment::Right),
            Cell::new(stats.read_errors + stats.write_errors).set_alignment(CellAlignment::Right),
            Cell::new(stats.reply_errors).set_alignment(CellAlignment::Right),
            Cell::new(stats.dangling).set_alignment(CellAlignment::Right),
            Cell::new(format!("{0:.2}", iteration_time.as_secs_f64()))
                .set_alignment(CellAlignment::Right),
        ]);

        // update request latencies table
        let mut request_histogram = Histogram::new();
        for latency in stats.reply_latencies.iter() {
            let ms = u64::try_from(latency.as_millis()).unwrap_or(u64::MAX);
            request_histogram.increment(ms).unwrap();
        }
        // this is an approximation only (since there are other things going on, but
        // since there are far more requests than anything else, it should be fairly ok)
        let request_throughput = stats.reply_latencies.len() as f64 / iteration_time.as_secs_f64();

        request_table.add_row(vec![
            Cell::new(peers).set_alignment(CellAlignment::Right),
            Cell::new(MAX_VALID_MESSAGES).set_alignment(CellAlignment::Right),
            Cell::new(request_histogram.minimum().unwrap_or_default())
                .set_alignment(CellAlignment::Right),
            Cell::new(request_histogram.maximum().unwrap_or_default())
                .set_alignment(CellAlignment::Right),
            Cell::new(request_histogram.stddev().unwrap_or_default())
                .set_alignment(CellAlignment::Right),
            Cell::new(
                request_histogram
                    .percentile(10.0)
                    .unwrap_or_default()
                    .to_string(),
            )
            .set_alignment(CellAlignment::Right),
            Cell::new(
                request_histogram
                    .percentile(50.0)
                    .unwrap_or_default()
                    .to_string(),
            )
            .set_alignment(CellAlignment::Right),
            Cell::new(
                request_histogram
                    .percentile(75.0)
                    .unwrap_or_default()
                    .to_string(),
            )
            .set_alignment(CellAlignment::Right),
            Cell::new(
                request_histogram
                    .percentile(90.0)
                    .unwrap_or_default()
                    .to_string(),
            )
            .set_alignment(CellAlignment::Right),
            Cell::new(
                request_histogram
                    .percentile(99.0)
                    .unwrap_or_default()
                    .to_string(),
            )
            .set_alignment(CellAlignment::Right),
            Cell::new(format!("{0:.2}", request_throughput)).set_alignment(CellAlignment::Right),
        ]);
    }

    // Display tables
    println!("Stats\n{}\n", stats_table);
    println!("Request latencies\n{}\n", request_table);

    node.stop().await;
}

// A list of valid queries and their expected responses
//
// This list is intentionally kept small - only simple and working
// query / response pairs are implemented.
fn valid_queries_responses() -> Vec<(Message, Message)> {
    let nonce = Nonce::default();

    let block_1 = Block::testnet_1();
    let block_2 = Block::testnet_2();

    vec![
        (Message::Ping(nonce), Message::Pong(nonce)),
        (
            Message::GetHeaders(LocatorHashes::new(
                vec![block_1.double_sha256().unwrap()],
                Hash::zeroed(),
            )),
            Message::Headers(Headers::new(vec![block_2.header.clone()])),
        ),
        (
            Message::GetBlocks(LocatorHashes::new(
                vec![block_1.double_sha256().unwrap()],
                Hash::zeroed(),
            )),
            Message::Inv(Inv::new(vec![block_2.inv_hash()])),
        ),
        (
            Message::GetData(Inv::new(vec![block_1.inv_hash()])),
            Message::Block(Box::new(block_1)),
        ),
    ]
}

fn generate_corrupt_messages(rng: &mut ChaCha8Rng, n: usize) -> Vec<Vec<u8>> {
    let message_pool = default_fuzz_messages();
    // generate a variety of corrupt messages and select n of them at random
    let mut possible_payloads = Vec::with_capacity(n * 6);
    possible_payloads.append(&mut zeroes(rng, n));
    possible_payloads.append(&mut slightly_corrupted_messages(rng, n, &message_pool));
    possible_payloads.append(&mut encode_messages_and_corrupt_checksum(
        rng,
        n,
        &message_pool,
    ));
    possible_payloads.append(&mut encode_messages_and_corrupt_body_length_field(
        rng,
        n,
        &message_pool,
    ));
    possible_payloads.append(&mut random_bytes(rng, n));

    let random_payloads = metadata_compliant_random_bytes(rng, n, &COMMANDS_WITH_PAYLOADS);
    for (header, payload) in random_payloads {
        let mut buffer = Vec::new();

        header.encode(&mut buffer).unwrap();
        buffer.write_all(&payload).unwrap();

        possible_payloads.push(buffer);
    }

    // remove payloads that ended up being valid
    possible_payloads.retain(|x| !is_valid_message_bytes(&mut std::io::Cursor::new(x)));

    possible_payloads.choose_multiple(rng, n).cloned().collect()
}

fn is_valid_message_bytes(bytes: &mut std::io::Cursor<&[u8]>) -> bool {
    let header = match MessageHeader::decode(bytes) {
        Ok(header) => header,
        Err(_) => return false,
    };

    // check magic
    if header.magic != MAGIC {
        return false;
    }

    Message::decode(header.command, bytes).is_ok()
}

async fn simulate_peer(
    node_addr: SocketAddr,
    event_tx: Sender<Event>,
    message_pairs: Vec<(Message, Message)>,
    corrupt_message: Vec<u8>,
) {
    // handshake
    let timer = tokio::time::Instant::now();
    let mut stream = match initiate_handshake(node_addr).await {
        Ok(stream) => {
            let _ = event_tx
                .send(Event::HandshakeEstablished(timer.elapsed()))
                .await;
            stream
        }
        Err(err) if is_rejection_error(&err) => {
            let _ = event_tx.send(Event::HandshakeRejected).await;
            let _ = event_tx.send(Event::Complete).await;
            return;
        }
        Err(err) => {
            let _ = event_tx.send(Event::HandshakeError(err)).await;
            let _ = event_tx.send(Event::Complete).await;
            return;
        }
    };

    // send the valid messages and validate the response
    let filter = MessageFilter::with_all_auto_reply();
    for (msg, expected) in message_pairs {
        match msg.write_to_stream(&mut stream).await {
            Ok(_) => {}
            Err(err) if is_termination_error(&err) => {
                dbg!(err);
                let _ = event_tx.send(Event::Dropped).await;
                let _ = event_tx.send(Event::Complete).await;
                return;
            }
            Err(err) => {
                let _ = event_tx.send(Event::WriteError(err)).await;
                let _ = event_tx.send(Event::Complete).await;
                return;
            }
        }
        let timer = tokio::time::Instant::now();
        match filter.read_from_stream(&mut stream).await {
            Ok(reply) if reply == expected => {
                let _ = event_tx.send(Event::ValidReply(timer.elapsed())).await;
            }
            Ok(reply) => {
                let _ = event_tx
                    .send(Event::BadReply(expected.into(), reply.into()))
                    .await;
                let _ = event_tx.send(Event::Complete).await;
                return;
            }
            Err(err) if is_termination_error(&err) => {
                let _ = event_tx.send(Event::Dropped).await;
                let _ = event_tx.send(Event::Complete).await;
                return;
            }
            Err(err) => {
                let _ = event_tx.send(Event::ReadError(err)).await;
                let _ = event_tx.send(Event::Complete).await;
                return;
            }
        }
    }

    // send the corrupt message and expect the connection to be terminated
    match tokio::io::AsyncWriteExt::write_all(&mut stream, &corrupt_message).await {
        Ok(_) => {}
        Err(err) if is_termination_error(&err) => {
            let _ = event_tx.send(Event::Dropped).await;
            let _ = event_tx.send(Event::Complete).await;
            return;
        }
        Err(err) => {
            let _ = event_tx.send(Event::WriteError(err)).await;
            let _ = event_tx.send(Event::Complete).await;
            return;
        }
    }
    //  check for termination by sending a ping -> pong (should result in a terminated connection prior to the pong)
    let nonce = Nonce::default();
    match Message::Ping(nonce).write_to_stream(&mut stream).await {
        Ok(_) => {}
        Err(err) if is_termination_error(&err) => {
            let _ = event_tx.send(Event::Terminated).await;
            let _ = event_tx.send(Event::Complete).await;
            return;
        }
        Err(err) => {
            let _ = event_tx.send(Event::WriteError(err)).await;
            let _ = event_tx.send(Event::Complete).await;
            return;
        }
    }
    match filter.read_from_stream(&mut stream).await {
        Ok(Message::Pong(rx_nonce)) if nonce == rx_nonce => {
            let _ = event_tx.send(Event::IgnoredCorrupt(corrupt_message)).await;
        }
        Ok(Message::Reject(..)) => {
            let _ = event_tx.send(Event::RejectedCorrupt).await;
        }
        Ok(message) => {
            let _ = event_tx.send(Event::RepliedToCorrupt(message.into())).await;
        }
        Err(err) if is_termination_error(&err) => {
            let _ = event_tx.send(Event::Terminated).await;
        }
        Err(err) => {
            let _ = event_tx.send(Event::ReadError(err)).await;
        }
    }
    let _ = event_tx.send(Event::Complete).await;
}

async fn manage_events(
    mut event_rx: tokio::sync::mpsc::Receiver<Event>,
    peer_count: usize,
) -> Stats {
    let mut stats = Stats::default();

    let mut peers_complete = 0;

    const EVENT_TIMEOUT: Duration = Duration::from_secs(10);

    while peers_complete < peer_count {
        match tokio::time::timeout(EVENT_TIMEOUT, event_rx.recv()).await {
            Ok(Some(event)) => {
                use Event::*;
                match event {
                    HandshakeEstablished(latency) => {
                        stats.handshake_accepted += 1;
                        stats.current_connections += 1;
                        stats.max_active_connections =
                            stats.max_active_connections.max(stats.current_connections);
                        stats.handshake_latencies.push(latency);
                    }
                    HandshakeRejected => {
                        stats.handshake_rejected += 1;
                    }
                    HandshakeError(err) => {
                        stats.handshake_errors += 1;
                        println!("Handshake error: {}", err);
                    }
                    ValidReply(latency) => stats.reply_latencies.push(latency),
                    BadReply(expected, reply) => {
                        stats.reply_errors += 1;
                        stats.current_connections -= 1;
                        println!("Bad reply!\nexpected: {:?}\n\ngot: {:?}", expected, reply);
                    }
                    WriteError(err) => {
                        stats.write_errors += 1;
                        stats.current_connections -= 1;
                        println!("Write error: {}", err);
                    }
                    ReadError(err) => {
                        stats.read_errors += 1;
                        stats.current_connections -= 1;
                        println!("Read error: {}", err);
                    }
                    Dropped => {
                        stats.peers_dropped += 1;
                        stats.current_connections -= 1;
                    }
                    Terminated => {
                        stats.current_connections -= 1;
                        stats.corrupt_terminated += 1;
                    }
                    RepliedToCorrupt(_msg) => {
                        stats.reply_instead_of_termination += 1;
                        stats.current_connections -= 1;
                    }
                    IgnoredCorrupt(_bytes) => {
                        stats.corrupt_ignored += 1;
                        stats.current_connections -= 1;
                    }
                    RejectedCorrupt => {
                        stats.corrupt_rejected += 1;
                        stats.current_connections -= 1;
                    }
                    Complete => peers_complete += 1,
                }
            }
            Ok(None) | Err(_) => {
                stats.dangling = (peer_count - peers_complete) as u16;
                println!(
                    "No events received for {} seconds, exiting with {} of {} peers unaccounted for",
                    EVENT_TIMEOUT.as_secs(),
                    stats.dangling,
                    peer_count
                );
                break;
            }
        }
    }

    stats
}
