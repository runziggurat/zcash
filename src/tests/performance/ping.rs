use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Cell, CellAlignment, Table};
use histogram::Histogram;
use std::convert::TryFrom;
use tokio::time::{timeout, Duration};

use crate::{
    helpers::initiate_handshake,
    protocol::{
        message::{filter::MessageFilter, Message},
        payload::Nonce,
    },
    setup::{
        config::new_local_addr,
        node::{Action, Node},
    },
};

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn ping_pong_latency() {
    // ZG-PERFORMANCE-001, Ping-Pong latency
    //
    // The node behaves as expected under load from other peers.
    //
    // We test the overall performance of a node's Ping-Pong latency.
    //
    // Note: This test does not assert any requirements, but requires manual inspection
    //       of the results table. This is because the results will rely on the machine
    //       running the test.
    //
    // ZCashd: appears to perform well.
    //
    // Zebra: starts choking on 30 concurrent peers, this is likely due
    //        to the DoS behaviour of spamming GetAddr and GetData.
    //
    // Example test result (with percentile latencies):
    //  *NOTE* run with `cargo test --release tests::performance::ping::ping_pong_latency -- --nocapture`
    //
    //  ZCashd
    //
    //  ╭───────┬───────┬──────────┬──────────┬─────────────┬──────────┬──────────┬──────────┬──────────┬──────────┬──────────┬──────────╮
    //  │ Peers ┆ Pings ┆ Min (ms) ┆ Max (ms) ┆ stddev (ms) ┆ 10% (ms) ┆ 50% (ms) ┆ 75% (ms) ┆ 90% (ms) ┆ 99% (ms) ┆ Time (s) ┆  Ping/s  │
    //  ╞═══════╪═══════╪══════════╪══════════╪═════════════╪══════════╪══════════╪══════════╪══════════╪══════════╪══════════╪══════════╡
    //  │     1 ┆  1000 ┆        0 ┆       50 ┆           4 ┆        0 ┆        0 ┆        0 ┆        0 ┆        0 ┆     0.39 ┆  2547.19 │
    //  ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┤
    //  │    10 ┆  1000 ┆        0 ┆       60 ┆           4 ┆        0 ┆        0 ┆        0 ┆        0 ┆       18 ┆     0.90 ┆ 11156.55 │
    //  ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┤
    //  │    20 ┆  1000 ┆        0 ┆      101 ┆           5 ┆        0 ┆        0 ┆        0 ┆        1 ┆        9 ┆     1.35 ┆ 14767.51 │
    //  ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┤
    //  │    30 ┆  1000 ┆        0 ┆       60 ┆           5 ┆        0 ┆        1 ┆        1 ┆        1 ┆        5 ┆     1.69 ┆ 17700.00 │
    //  ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┤
    //  │    40 ┆  1000 ┆        0 ┆       61 ┆           4 ┆        1 ┆        1 ┆        1 ┆        2 ┆        4 ┆     2.20 ┆ 18207.26 │
    //  ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┤
    //  │    50 ┆  1000 ┆        0 ┆       62 ┆           5 ┆        1 ┆        2 ┆        2 ┆        2 ┆       13 ┆     2.77 ┆ 18031.54 │
    //  ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┤
    //  │    60 ┆  1000 ┆        0 ┆       69 ┆           5 ┆        2 ┆        2 ┆        2 ┆        3 ┆       14 ┆     3.30 ┆ 18171.79 │
    //  ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┤
    //  │    70 ┆  1000 ┆        0 ┆       62 ┆           5 ┆        2 ┆        3 ┆        3 ┆        3 ┆        9 ┆     3.66 ┆ 19132.38 │
    //  ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┤
    //  │    80 ┆  1000 ┆        0 ┆       69 ┆           5 ┆        2 ┆        3 ┆        3 ┆        4 ┆       10 ┆     4.00 ┆ 20000.38 │
    //  ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┤
    //  │    90 ┆  1000 ┆        0 ┆       62 ┆           5 ┆        3 ┆        3 ┆        3 ┆        4 ┆        7 ┆     4.15 ┆ 21708.00 │
    //  ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┤
    //  │   100 ┆  1000 ┆        0 ┆       68 ┆           5 ┆        3 ┆        4 ┆        4 ┆        4 ┆        8 ┆     4.65 ┆ 21511.20 │
    //  ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┤
    //  │   200 ┆  1000 ┆        0 ┆      100 ┆           5 ┆        7 ┆        8 ┆        8 ┆        9 ┆       43 ┆     9.18 ┆ 21781.61 │
    //  ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┤
    //  │   300 ┆  1000 ┆        0 ┆       81 ┆           6 ┆       11 ┆       12 ┆       13 ┆       14 ┆       52 ┆    13.45 ┆ 22312.15 │
    //  ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┤
    //  │   500 ┆  1000 ┆        0 ┆      107 ┆           7 ┆       18 ┆       20 ┆       21 ┆       22 ┆       61 ┆    21.79 ┆ 22943.68 │
    //  ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┤
    //  │   750 ┆  1000 ┆        0 ┆      119 ┆           8 ┆       27 ┆       30 ┆       32 ┆       34 ┆       63 ┆    32.39 ┆ 23156.53 │
    //  ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┤
    //  │   800 ┆  1000 ┆        0 ┆      137 ┆           8 ┆       29 ┆       32 ┆       33 ┆       36 ┆       68 ┆    34.53 ┆ 23167.10 │
    //  ╰───────┴───────┴──────────┴──────────┴─────────────┴──────────┴──────────┴──────────┴──────────┴──────────┴──────────┴──────────╯
    //
    //  zebra
    //  ╭───────┬───────┬──────────┬──────────┬─────────────┬──────────┬──────────┬──────────┬──────────┬──────────┬──────────┬─────────╮
    //  │ Peers ┆ Pings ┆ Min (ms) ┆ Max (ms) ┆ stddev (ms) ┆ 10% (ms) ┆ 50% (ms) ┆ 75% (ms) ┆ 90% (ms) ┆ 99% (ms) ┆ Time (s) ┆  Ping/s │
    //  ╞═══════╪═══════╪══════════╪══════════╪═════════════╪══════════╪══════════╪══════════╪══════════╪══════════╪══════════╪═════════╡
    //  │     1 ┆  1000 ┆        0 ┆        4 ┆           1 ┆        1 ┆        1 ┆        1 ┆        1 ┆        2 ┆     1.79 ┆  559.10 │
    //  ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┤
    //  │    10 ┆  1000 ┆        0 ┆      128 ┆           6 ┆        3 ┆        4 ┆        5 ┆        6 ┆       47 ┆     5.77 ┆ 1733.04 │
    //  ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┤
    //  │    20 ┆  1000 ┆        0 ┆       79 ┆           7 ┆       19 ┆       20 ┆       20 ┆       21 ┆       44 ┆    21.44 ┆  932.71 │
    //  ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┤
    //  │    30 ┆  1000 ┆        0 ┆      178 ┆          12 ┆       49 ┆       51 ┆       53 ┆       55 ┆      104 ┆    54.24 ┆  553.09 │
    //  ╰───────┴───────┴──────────┴──────────┴─────────────┴──────────┴──────────┴──────────┴──────────┴──────────┴──────────┴─────────╯

    // number of pings to send per peer
    const PINGS: usize = 1000;
    const PING_TIMEOUT: Duration = Duration::from_millis(200);
    // number of concurrent peers to test (zcashd hardcaps `max_peers` to 873 on my machine)
    let peer_counts = vec![
        1, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 200, 300, 500, 750, 800,
    ];

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("Peers").set_alignment(CellAlignment::Center),
            Cell::new("Pings").set_alignment(CellAlignment::Center),
            Cell::new("Min (ms)").set_alignment(CellAlignment::Center),
            Cell::new("Max (ms)").set_alignment(CellAlignment::Center),
            Cell::new("stddev (ms)").set_alignment(CellAlignment::Center),
            Cell::new("10% (ms)").set_alignment(CellAlignment::Center),
            Cell::new("50% (ms)").set_alignment(CellAlignment::Center),
            Cell::new("75% (ms)").set_alignment(CellAlignment::Center),
            Cell::new("90% (ms)").set_alignment(CellAlignment::Center),
            Cell::new("99% (ms)").set_alignment(CellAlignment::Center),
            Cell::new("Time (s)").set_alignment(CellAlignment::Center),
            Cell::new("Ping/s").set_alignment(CellAlignment::Center),
        ]);

    // start node, with max peers set so that our peers should
    // never be rejected.
    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection)
        .max_peers(peer_counts.iter().max().unwrap() * 2 + 10)
        .start()
        .await;
    let node_addr = node.addr();

    for peers in peer_counts {
        // create N peer nodes which send M ping's as fast as possible
        let mut peer_handles = Vec::with_capacity(peers);

        let test_start = tokio::time::Instant::now();

        for _ in 0..peers {
            peer_handles.push(tokio::spawn(async move {
                let mut stream = initiate_handshake(node_addr).await.unwrap();

                let filter = MessageFilter::with_all_auto_reply();

                let mut latencies = Vec::with_capacity(PINGS);
                for _ in 0..PINGS {
                    let nonce = Nonce::default();
                    let expected = Message::Pong(nonce);
                    Message::Ping(nonce)
                        .write_to_stream(&mut stream)
                        .await
                        .unwrap();
                    let now = tokio::time::Instant::now();
                    loop {
                        match timeout(PING_TIMEOUT, filter.read_from_stream(&mut stream)).await {
                            Err(_elapsed) => latencies.push(PING_TIMEOUT),
                            Ok(Ok(message)) if message == expected => latencies.push(now.elapsed()),
                            // If the nonce doesn't match then we treat it as a response to an already timed out Ping
                            // (which has already been handled, so we skip it).
                            Ok(Ok(Message::Pong(_))) => continue,
                            Ok(result) => {
                                panic!("Failed to receive {:?}, got {:?}", expected, result)
                            }
                        }

                        break;
                    }
                }

                latencies
            }));
        }

        // wait for peers to complete
        let mut peer_latencies = Vec::with_capacity(peers);
        for handle in peer_handles {
            peer_latencies.push(handle.await.unwrap());
        }

        let time_taken_secs = test_start.elapsed().as_secs_f32();
        let throughput = (peers * PINGS) as f32 / time_taken_secs;

        // Tally-up latencies
        let mut histogram = Histogram::new();
        for peer in peer_latencies {
            for duration in peer {
                let ms = u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
                histogram.increment(ms).unwrap();
            }
        }

        // add stats to table display
        table.add_row(vec![
            Cell::new(peers.to_string()).set_alignment(CellAlignment::Right),
            Cell::new(PINGS.to_string()).set_alignment(CellAlignment::Right),
            Cell::new(histogram.minimum().unwrap().to_string()).set_alignment(CellAlignment::Right),
            Cell::new(histogram.maximum().unwrap().to_string()).set_alignment(CellAlignment::Right),
            Cell::new(histogram.stddev().unwrap().to_string()).set_alignment(CellAlignment::Right),
            Cell::new(histogram.percentile(10.0).unwrap().to_string())
                .set_alignment(CellAlignment::Right),
            Cell::new(histogram.percentile(50.0).unwrap().to_string())
                .set_alignment(CellAlignment::Right),
            Cell::new(histogram.percentile(75.0).unwrap().to_string())
                .set_alignment(CellAlignment::Right),
            Cell::new(histogram.percentile(90.0).unwrap().to_string())
                .set_alignment(CellAlignment::Right),
            Cell::new(histogram.percentile(99.0).unwrap().to_string())
                .set_alignment(CellAlignment::Right),
            Cell::new(format!("{0:.2}", time_taken_secs)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{0:.2}", throughput)).set_alignment(CellAlignment::Right),
        ]);
    }

    node.stop().await;

    // Display various percentiles
    println!("{}", table);
}
