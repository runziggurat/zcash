use std::collections::VecDeque;

use tokio::time::Duration;

use crate::{
    protocol::{
        message::Message,
        payload::{block::Block, Inv},
    },
    setup::node::{Action, Node},
    tools::{
        metrics::{
            recorder::TestMetrics,
            tables::{duration_as_ms, RequestStats, RequestsTable},
        },
        synthetic_node::SyntheticNode,
    },
};

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn throughput() {
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
    //
    // Zebra: Does not support block seeding and therefore cannot run this test.
    //
    // Example test result (with percentile latencies):
    //  *NOTE* run with `cargo test --release tests::performance::getdata_blocks::throughput -- --nocapture`
    //
    //  ZCashd
    //
    // ┌───────┬──────────┬──────────┬──────────┬──────────────┬──────────┬──────────┬──────────┬──────────┬──────────┬──────────────┬──────────┬────────────┐
    // │ peers │ requests │ min (ms) │ max (ms) │ std dev (ms) │ 10% (ms) │ 50% (ms) │ 75% (ms) │ 90% (ms) │ 99% (ms) │ completion % │ time (s) │ requests/s │
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │      1│       100│         0│       100│            12│         0│         0│         0│         0│       100│        100.00│      0.20│      492.77│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │     10│       100│         0│        56│            18│         3│        53│        53│        54│        55│        100.00│      4.68│      213.90│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │     20│       100│         1│        58│            25│         6│        56│        57│        57│        58│        100.00│      3.91│      511.67│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │     30│       100│         1│       101│            26│        10│        60│        60│        60│        61│        100.00│      4.16│      721.91│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │     40│       100│         0│        66│            26│        13│        63│        64│        64│        65│        100.00│      4.12│      970.71│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │     50│       100│         0│        69│            26│        17│        67│        67│        68│        68│        100.00│      4.59│     1089.24│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │     60│       100│         0│        71│            26│        20│        70│        70│        71│        71│        100.00│      5.09│     1178.46│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │     70│       100│         1│        76│            24│        23│        74│        74│        75│        76│        100.00│      5.97│     1172.73│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │     80│       100│         1│        80│            25│        27│        77│        78│        78│        79│        100.00│      6.33│     1264.15│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │     90│       100│         2│        82│            25│        30│        31│        81│        81│        82│        100.00│      5.01│     1796.84│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │    100│       100│         0│        86│            21│        34│        34│        35│        84│        85│        100.00│      4.50│     2223.27│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │    200│       100│         1│       241│            27│        68│        85│       119│       120│       121│        100.00│      9.67│     2067.24│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │    300│       100│         4│       312│            26│       103│       105│       154│       156│       157│        100.00│     12.25│     2448.54│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │    500│       100│         1│       460│            30│       172│       176│       178│       227│       231│        100.00│     19.19│     2605.02│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │    750│       100│         1│       617│            37│       263│       269│       273│       279│       328│        100.00│     27.43│     2734.17│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │    800│       100│         0│       667│            40│       280│       284│       288│       292│       358│        100.00│     29.06│     2752.93│
    // └───────┴──────────┴──────────┴──────────┴──────────────┴──────────┴──────────┴──────────┴──────────┴──────────┴──────────────┴──────────┴────────────┘

    // number of requests to send per peer
    const REQUESTS: usize = 100;
    const REQUEST_TIMEOUT: Duration = Duration::from_secs(1);
    // number of concurrent peers to test (zcashd hardcaps `max_peers` to 873 on my machine)
    let synth_counts = vec![
        1, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 200, 300, 500, 750, 800,
    ];

    let mut table = RequestsTable::default();
    const METRIC_LATENCY: &str = "block_test_latency";

    // Start node seeded with initial testnet blocks,
    // with max peers set so that our peers should never be rejected.
    let mut node = Node::new().unwrap();
    node.initial_action(Action::SeedWithTestnetBlocks(11))
        .max_peers(synth_counts.iter().max().unwrap() * 2 + 10)
        .start()
        .await
        .unwrap();
    let node_addr = node.addr();

    for synth_count in synth_counts {
        // setup metrics recorder
        let test_metrics = TestMetrics::default();
        // register metrics
        metrics::register_histogram!(METRIC_LATENCY);

        // create N peer nodes which send M requests's as fast as possible
        let mut synth_handles = Vec::with_capacity(synth_count);

        let test_start = tokio::time::Instant::now();

        for _ in 0..synth_count {
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

            synth_handles.push(tokio::spawn(async move {
                let mut synth_node = SyntheticNode::builder()
                    .with_full_handshake()
                    .with_all_auto_reply()
                    .build()
                    .await
                    .unwrap();

                synth_node.connect(node_addr).await.unwrap();

                for i in 0..REQUESTS {
                    let (request, expected) = &requests[i % requests.len()];
                    synth_node
                        .send_direct_message(node_addr, request.clone())
                        .unwrap();
                    let now = tokio::time::Instant::now();
                    match synth_node.recv_message_timeout(REQUEST_TIMEOUT).await {
                        Err(_timeout) => break,
                        Ok((_, Message::Block(block))) if &block == expected => {
                            metrics::histogram!(METRIC_LATENCY, duration_as_ms(now.elapsed()));
                        }
                        Ok((_, bad_reply)) => {
                            panic!("Failed to receive Block, got {:?}", bad_reply);
                        }
                    }
                }
            }));
        }

        // wait for peers to complete
        for handle in synth_handles {
            handle.await.unwrap();
        }

        let time_taken_secs = test_start.elapsed().as_secs_f64();

        if let Some(latencies) = test_metrics.construct_histogram(METRIC_LATENCY) {
            if latencies.entries() >= 1 {
                // add stats to table display
                table.add_row(RequestStats::new(
                    synth_count as u16,
                    REQUESTS as u16,
                    latencies,
                    time_taken_secs,
                ));
            }
        }
    }

    node.stop().unwrap();

    // Display various percentiles
    println!("{}", table);
}
