use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Cell, CellAlignment, Table};
use histogram::Histogram;
use std::{collections::VecDeque, convert::TryFrom};
use tokio::time::{timeout, Duration};

use crate::{
    helpers::initiate_handshake,
    protocol::{
        message::{Message, MessageFilter},
        payload::{block::Block, Inv},
    },
    setup::{
        config::new_local_addr,
        node::{Action, Node},
    },
};

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn getdata_blocks_latency() {
    // ZG-PERFORMANCE-001, GetData-Block latency
    //
    // The node behaves as expected under load from other peers.
    //
    // We test the overall performance of a node's GetData-Block latency.
    //
    // Note: This test does not assert any requirements, but requires manual inspection
    //       of the results table. This is because the results will rely on the machine
    //       running the test.
    //
    // ZCashd: Strange, small slow-down as soon as multiple peers are present.
    //         Otherwise performs consistently up to around 200 concurrent peers.
    //
    // Zebra: Chokes right out of the gate, likely to the DoS behaviour of spamming GetAddr and GetData.
    //        The spam can be seem by enabling logging on the filter.
    //
    // Example test result (with percentile latencies):
    //  *NOTE* run with `cargo test --release tests::performance::blocks::getdata_blocks_latency -- --nocapture`
    //
    //  ZCashd
    //
    //  ╭───────┬──────────┬──────────┬──────────┬─────────────┬──────────┬──────────┬──────────┬──────────┬──────────┬──────────┬───────────╮
    //  │ Peers ┆ Requests ┆ Min (ms) ┆ Max (ms) ┆ stddev (ms) ┆ 10% (ms) ┆ 50% (ms) ┆ 75% (ms) ┆ 90% (ms) ┆ 99% (ms) ┆ Time (s) ┆ Request/s │
    //  ╞═══════╪══════════╪══════════╪══════════╪═════════════╪══════════╪══════════╪══════════╪══════════╪══════════╪══════════╪═══════════╡
    //  │     1 ┆      100 ┆        0 ┆       50 ┆           5 ┆        0 ┆        0 ┆        0 ┆        0 ┆       50 ┆     0.10 ┆   1016.30 │
    //  ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
    //  │    10 ┆      100 ┆        0 ┆       56 ┆          12 ┆       53 ┆       53 ┆       53 ┆       54 ┆       55 ┆     5.19 ┆    192.71 │
    //  ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
    //  │    20 ┆      100 ┆        0 ┆       66 ┆          26 ┆        6 ┆       56 ┆       57 ┆       58 ┆       65 ┆     3.71 ┆    539.12 │
    //  ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
    //  │    30 ┆      100 ┆        0 ┆      109 ┆          24 ┆       10 ┆       60 ┆       60 ┆       61 ┆       63 ┆     4.31 ┆    696.62 │
    //  ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
    //  │    40 ┆      100 ┆        0 ┆       67 ┆          25 ┆       13 ┆       63 ┆       63 ┆       64 ┆       67 ┆     4.35 ┆    920.02 │
    //  ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
    //  │    50 ┆      100 ┆        0 ┆       99 ┆          25 ┆       16 ┆       67 ┆       67 ┆       68 ┆       73 ┆     5.03 ┆    994.37 │
    //  ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
    //  │    60 ┆      100 ┆        0 ┆      128 ┆          25 ┆       20 ┆       44 ┆       70 ┆       71 ┆       73 ┆     4.82 ┆   1244.89 │
    //  ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
    //  │    70 ┆      100 ┆        0 ┆      121 ┆          20 ┆       23 ┆       23 ┆       39 ┆       73 ┆       82 ┆     3.47 ┆   2019.06 │
    //  ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
    //  │    80 ┆      100 ┆        0 ┆      112 ┆          22 ┆       26 ┆       27 ┆       56 ┆       77 ┆       78 ┆     4.30 ┆   1859.31 │
    //  ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
    //  │    90 ┆      100 ┆        0 ┆      161 ┆          25 ┆       30 ┆       32 ┆       80 ┆       81 ┆       98 ┆     5.24 ┆   1716.46 │
    //  ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
    //  │   100 ┆      100 ┆        0 ┆      118 ┆          25 ┆       33 ┆       35 ┆       84 ┆       84 ┆       86 ┆     5.69 ┆   1756.27 │
    //  ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
    //  │   200 ┆      100 ┆        3 ┆      187 ┆          24 ┆       67 ┆       69 ┆      118 ┆      119 ┆      123 ┆     8.52 ┆   2347.92 │
    //  ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
    //  │   300 ┆      100 ┆        1 ┆      303 ┆          21 ┆      102 ┆      104 ┆      105 ┆      153 ┆      157 ┆    11.12 ┆   2698.96 │
    //  ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
    //  │   500 ┆      100 ┆        0 ┆      460 ┆          26 ┆      169 ┆      174 ┆      175 ┆      178 ┆      227 ┆    18.09 ┆   2763.85 │
    //  ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
    //  │   750 ┆      100 ┆        1 ┆      550 ┆          33 ┆      261 ┆      265 ┆      269 ┆      273 ┆      284 ┆    27.05 ┆   2772.65 │
    //  ├╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
    //  │   800 ┆      100 ┆        1 ┆      608 ┆          38 ┆      267 ┆      287 ┆      292 ┆      297 ┆      315 ┆    29.16 ┆   2743.55 │
    //  ╰───────┴──────────┴──────────┴──────────┴─────────────┴──────────┴──────────┴──────────┴──────────┴──────────┴──────────┴───────────╯
    //
    //  zebra
    //  ╭───────┬──────────┬──────────┬──────────┬─────────────┬──────────┬──────────┬──────────┬──────────┬──────────┬──────────┬───────────╮
    //  │ Peers ┆ Requests ┆ Min (ms) ┆ Max (ms) ┆ stddev (ms) ┆ 10% (ms) ┆ 50% (ms) ┆ 75% (ms) ┆ 90% (ms) ┆ 99% (ms) ┆ Time (s) ┆ Request/s │
    //  ╞═══════╪══════════╪══════════╪══════════╪═════════════╪══════════╪══════════╪══════════╪══════════╪══════════╪══════════╪═══════════╡
    //  │     1 ┆      100 ┆      100 ┆      100 ┆           0 ┆      100 ┆      100 ┆      100 ┆      100 ┆      100 ┆    10.11 ┆      9.89 │
    //  ╰───────┴──────────┴──────────┴──────────┴─────────────┴──────────┴──────────┴──────────┴──────────┴──────────┴──────────┴───────────╯

    // number of requests to send per peer
    const REQUESTS: usize = 100;
    const REQUEST_TIMEOUT: Duration = Duration::from_millis(100);
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
            Cell::new("Requests").set_alignment(CellAlignment::Center),
            Cell::new("Min (ms)").set_alignment(CellAlignment::Center),
            Cell::new("Max (ms)").set_alignment(CellAlignment::Center),
            Cell::new("stddev (ms)").set_alignment(CellAlignment::Center),
            Cell::new("10% (ms)").set_alignment(CellAlignment::Center),
            Cell::new("50% (ms)").set_alignment(CellAlignment::Center),
            Cell::new("75% (ms)").set_alignment(CellAlignment::Center),
            Cell::new("90% (ms)").set_alignment(CellAlignment::Center),
            Cell::new("99% (ms)").set_alignment(CellAlignment::Center),
            Cell::new("Time (s)").set_alignment(CellAlignment::Center),
            Cell::new("Request/s").set_alignment(CellAlignment::Center),
        ]);

    // Start node seeded with initial testnet blocks,
    // with max peers set so that our peers should never be rejected.
    let mut node: Node = Default::default();
    node.initial_action(Action::SeedWithTestnetBlocks {
        socket_addr: new_local_addr(),
        block_count: 3,
    })
    .max_peers(peer_counts.iter().max().unwrap() * 2 + 10)
    .start()
    .await;
    let node_addr = node.addr();

    for peers in peer_counts {
        // create N peer nodes which send M requests's as fast as possible
        let mut peer_handles = Vec::with_capacity(peers);

        let test_start = tokio::time::Instant::now();

        for _ in 0..peers {
            // We want different blocks for consecutive requests, in order to determine if the node
            // has skipped a request or to tell if the reply is in response to a timed out request.
            //
            // We also store the Block, in order to compare to the reply.
            let requests = Block::initial_testnet_blocks()
                .into_iter()
                .map(|block| {
                    (
                        Message::GetData(Inv::new(vec![block.inv_hash()])),
                        Box::new(block),
                    )
                })
                .collect::<VecDeque<_>>();

            peer_handles.push(tokio::spawn(async move {
                let mut stream = initiate_handshake(node_addr).await.unwrap();

                let filter = MessageFilter::with_all_auto_reply();

                let mut latencies = Vec::with_capacity(REQUESTS);
                for i in 0..REQUESTS {
                    let (request, expected) = &requests[i % requests.len()];
                    request.write_to_stream(&mut stream).await.unwrap();
                    let now = tokio::time::Instant::now();
                    loop {
                        match timeout(REQUEST_TIMEOUT, filter.read_from_stream(&mut stream)).await {
                            Err(_elapsed) => latencies.push(REQUEST_TIMEOUT),
                            Ok(Ok(Message::Block(block))) if &block == expected => {
                                latencies.push(now.elapsed())
                            }
                            // If the block doesn't match then we treat it as a response to an already timed out request
                            // (which has already been handled, so we skip it).
                            Ok(Ok(Message::Block(_))) => continue,
                            Ok(result) => {
                                panic!("Failed to receive Block, got {:?}", result)
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
        let throughput = (peers * REQUESTS) as f32 / time_taken_secs;

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
            Cell::new(REQUESTS.to_string()).set_alignment(CellAlignment::Right),
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
