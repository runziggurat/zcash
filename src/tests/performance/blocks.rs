use histogram::Histogram;
use std::{collections::VecDeque, convert::TryFrom};
use tokio::time::{timeout, Duration};

use crate::{
    helpers::initiate_handshake,
    protocol::{
        message::{filter::MessageFilter, Message},
        payload::{block::Block, Inv},
    },
    setup::node::{Action, Node},
    tests::performance::{RequestStats, RequestsTable},
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
    //
    // Zebra: Starts dropping connections from 200 peers onwards.
    //
    // Example test result (with percentile latencies):
    //  *NOTE* run with `cargo test --release tests::performance::blocks::getdata_blocks_latency -- --nocapture`
    //
    //  ZCashd
    //
    // ┌───────┬──────────┬──────────┬──────────┬──────────────┬──────────┬──────────┬──────────┬──────────┬──────────┬──────────┬────────────┐
    // │ peers │ requests │ min (ms) │ max (ms) │ std dev (ms) │ 10% (ms) │ 50% (ms) │ 75% (ms) │ 90% (ms) │ 99% (ms) │ time (s) │ requests/s │
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │      1│       100│         0│        50│             5│         0│         0│         0│         0│        50│      0.10│      994.76│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     10│       100│         0│        58│            24│         3│        53│        53│        53│        54│      3.83│      261.40│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     20│       100│         0│        58│            21│         6│        57│        57│        57│        57│      4.78│      418.59│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     30│       100│         0│        70│            24│         9│        15│        60│        60│        61│      3.44│      872.22│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     40│       100│         0│       100│            17│        13│        13│        29│        63│        64│      2.57│     1559.43│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     50│       100│         0│       100│            19│        16│        17│        38│        66│        68│      3.06│     1635.55│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     60│       100│         0│       100│            20│        20│        21│        43│        70│        71│      3.69│     1627.64│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     70│       100│         0│       100│            19│        23│        24│        40│        74│        75│      3.76│     1862.06│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     80│       100│         0│       100│            22│        26│        27│        53│        77│        83│      4.37│     1831.51│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     90│       100│         0│       100│            21│        30│        31│        54│        81│        82│      4.63│     1944.52│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │    100│       100│         0│       100│            24│        34│        36│        84│        85│        92│      5.45│     1835.68│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │    200│       100│         1│       144│            15│        69│        71│        80│       100│       100│      7.88│     2538.25│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │    300│       100│         1│       208│             8│       100│       100│       100│       100│       100│     11.14│     2693.10│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │    500│       100│         1│       246│            10│       100│       100│       100│       100│       100│     17.82│     2805.36│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │    750│       100│         0│       265│            17│        77│       100│       100│       100│       100│     11.57│     6481.19│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │    800│       100│         0│       227│            14│        93│       100│       100│       100│       100│     12.17│     6571.53│
    // └───────┴──────────┴──────────┴──────────┴──────────────┴──────────┴──────────┴──────────┴──────────┴──────────┴──────────┴────────────┘
    //
    //  zebra
    // ┌───────┬──────────┬──────────┬──────────┬──────────────┬──────────┬──────────┬──────────┬──────────┬──────────┬──────────┬────────────┐
    // │ peers │ requests │ min (ms) │ max (ms) │ std dev (ms) │ 10% (ms) │ 50% (ms) │ 75% (ms) │ 90% (ms) │ 99% (ms) │ time (s) │ requests/s │
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │      1│       100│       100│       100│             0│       100│       100│       100│       100│       100│     10.13│        9.87│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     10│       100│       100│       100│             0│       100│       100│       100│       100│       100│     10.13│       98.73│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     20│       100│       100│       100│             0│       100│       100│       100│       100│       100│     10.14│      197.31│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     30│       100│       100│       100│             0│       100│       100│       100│       100│       100│     10.14│      295.79│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     40│       100│       100│       100│             0│       100│       100│       100│       100│       100│     10.21│      391.87│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     50│       100│       100│       100│             0│       100│       100│       100│       100│       100│     10.16│      491.91│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     60│       100│       100│       100│             0│       100│       100│       100│       100│       100│     10.19│      588.56│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     70│       100│       100│       100│             0│       100│       100│       100│       100│       100│     10.17│      688.56│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     80│       100│       100│       100│             0│       100│       100│       100│       100│       100│     10.18│      786.12│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     90│       100│       100│       100│             0│       100│       100│       100│       100│       100│     10.18│      884.50│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │    100│       100│       100│       100│             0│       100│       100│       100│       100│       100│     10.18│      982.52│
    // └───────┴──────────┴──────────┴──────────┴──────────────┴──────────┴──────────┴──────────┴──────────┴──────────┴──────────┴────────────┘

    // number of requests to send per peer
    const REQUESTS: usize = 100;
    const REQUEST_TIMEOUT: Duration = Duration::from_millis(100);
    // number of concurrent peers to test (zcashd hardcaps `max_peers` to 873 on my machine)
    let peer_counts = vec![
        1, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 200, 300, 500, 750, 800,
    ];

    let mut table = RequestsTable::default();

    // Start node seeded with initial testnet blocks,
    // with max peers set so that our peers should never be rejected.
    let mut node: Node = Default::default();
    node.initial_action(Action::SeedWithTestnetBlocks(3))
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

        let time_taken_secs = test_start.elapsed().as_secs_f64();

        // Tally-up latencies
        let mut histogram = Histogram::new();
        for peer in peer_latencies {
            for duration in peer {
                let ms = u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
                histogram.increment(ms).unwrap();
            }
        }

        // add stats to table display
        table.add_row(RequestStats::new(
            peers as u16,
            REQUESTS as u16,
            histogram,
            time_taken_secs,
        ));
    }

    node.stop().await;

    // Display various percentiles
    println!("{}", table);
}
