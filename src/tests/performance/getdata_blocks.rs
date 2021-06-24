use std::collections::VecDeque;
use tokio::time::Duration;

use crate::{
    protocol::{
        message::Message,
        payload::{block::Block, Inv},
    },
    setup::node::{Action, Node},
    tests::{
        performance::{duration_as_ms, RequestStats, RequestsTable},
        simple_metrics::{self, enable_simple_recorder},
    },
    tools::synthetic_node::SyntheticNode,
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
    //  *NOTE* run with `cargo test --release tests::performance::getdata_blocks::latency -- --nocapture`
    //
    //  ZCashd
    //
    // ┌───────┬──────────┬──────────┬──────────┬──────────────┬──────────┬──────────┬──────────┬──────────┬──────────┬──────────┬────────────┐
    // │ peers │ requests │ min (ms) │ max (ms) │ std dev (ms) │ 10% (ms) │ 50% (ms) │ 75% (ms) │ 90% (ms) │ 99% (ms) │ time (s) │ requests/s │
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │      1│       100│         0│        50│             5│         0│         0│         0│         0│        50│      0.10│     1014.39│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     10│       100│         0│        56│            13│        53│        53│        54│        54│        55│      5.14│      194.44│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     20│       100│         1│        62│            20│         7│        57│        58│        59│        61│      4.95│      404.22│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     30│       100│         0│        69│            26│        10│        60│        61│        62│        66│      3.91│      766.97│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     40│       100│         1│        74│            23│        14│        63│        64│        65│        71│      5.07│      789.16│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     50│       100│         1│       101│            25│        17│        67│        68│        69│        74│      5.38│      930.19│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     60│       100│         0│       141│            26│        20│        23│        71│        71│        74│      4.64│     1291.94│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     70│       100│         1│        92│            26│        23│        73│        74│        75│        84│      5.26│     1329.99│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     80│       100│         1│       104│            26│        27│        77│        78│        79│        84│      5.62│     1422.46│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     90│       100│         1│        89│            26│        30│        34│        81│        81│        85│      5.57│     1616.57│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │    100│       100│         1│        86│            26│        33│        84│        84│        85│        86│      6.09│     1642.60│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │    200│       100│         0│       246│            27│        68│       118│       119│       122│       131│      9.93│     2014.71│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │    300│       100│         2│       330│            27│       103│       107│       155│       157│       181│     12.78│     2347.94│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │    500│       100│         2│       447│            29│       170│       173│       175│       222│       226│     18.29│     2733.24│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │    750│       100│         0│       577│            35│       262│       269│       275│       282│       312│     27.44│     2733.38│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │    800│       100│         1│       778│            45│       272│       298│       314│       332│       414│     30.96│     2583.62│
    // └───────┴──────────┴──────────┴──────────┴──────────────┴──────────┴──────────┴──────────┴──────────┴──────────┴──────────┴────────────┘

    // setup metrics recorder
    enable_simple_recorder().unwrap();

    // number of requests to send per peer
    const REQUESTS: usize = 100;
    const REQUEST_TIMEOUT: Duration = Duration::from_secs(1);
    // number of concurrent peers to test (zcashd hardcaps `max_peers` to 873 on my machine)
    let synth_counts = vec![
        1, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 200, 300, 500, 750, 800,
    ];

    let mut table = RequestsTable::default();
    const METRIC_NAME: &str = "block_test";

    // Start node seeded with initial testnet blocks,
    // with max peers set so that our peers should never be rejected.
    let mut node: Node = Default::default();
    node.initial_action(Action::SeedWithTestnetBlocks(3))
        .max_peers(synth_counts.iter().max().unwrap() * 2 + 10)
        .start()
        .await;
    let node_addr = node.addr();

    for synth_count in synth_counts {
        // clear and register metrics
        simple_metrics::clear();
        metrics::register_histogram!(METRIC_NAME);

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
                        .await
                        .unwrap();
                    let now = tokio::time::Instant::now();
                    loop {
                        match synth_node.recv_message_timeout(REQUEST_TIMEOUT).await {
                            Err(_timeout) => {
                                metrics::histogram!(METRIC_NAME, duration_as_ms(REQUEST_TIMEOUT))
                            }
                            Ok((_, Message::Block(block))) if &block == expected => {
                                metrics::histogram!(METRIC_NAME, duration_as_ms(now.elapsed()))
                            }
                            // If the block doesn't match then we treat it as a response to an already timed out request
                            // (which has already been handled, so we skip it).
                            Ok((_, Message::Block(_))) => continue,
                            Ok((_, message)) => {
                                panic!("Failed to receive Block, got {:?}", message)
                            }
                        }

                        break;
                    }
                }
            }));
        }

        // wait for peers to complete
        for handle in synth_handles {
            handle.await.unwrap();
        }

        let time_taken_secs = test_start.elapsed().as_secs_f64();

        // grab latencies from metrics recoder
        let latencies = simple_metrics::histograms()
            .lock()
            .get(&metrics::Key::from_name(METRIC_NAME))
            .unwrap()
            .value
            .clone();

        // add stats to table display
        table.add_row(RequestStats::new(
            synth_count as u16,
            REQUESTS as u16,
            latencies,
            time_taken_secs,
        ));
    }

    node.stop().await;

    // Display various percentiles
    println!("{}", table);
}
