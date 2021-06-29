use std::{net::SocketAddr, time::Duration};

use crate::{
    setup::node::{Action, Node},
    tests::{
        performance::{fmt_table, table_float_display},
        simple_metrics,
    },
    tools::synthetic_node::SyntheticNode,
};

use tabled::{Table, Tabled};
use tokio::sync::mpsc::Sender;

#[derive(Tabled, Default, Debug, Clone)]
struct Stats {
    #[header("\n max peers ")]
    pub max_peers: u16,
    #[header("\n peers ")]
    pub peers: u16,
    #[header(" connection \n accepted ")]
    pub accepted: u16,
    #[header(" connection \n rejected ")]
    pub rejected: u16,
    #[header(" connection \n terminated ")]
    pub terminated: u16,
    #[header(" connection \n error ")]
    pub conn_error: u16,
    #[header(" connection \n timed out ")]
    pub timed_out: u16,
    #[header("\n time (s) ")]
    #[field(display_with = "table_float_display")]
    pub time: f64,
}

impl Stats {
    fn new(max_peers: u16, peers: u16) -> Self {
        Self {
            max_peers,
            peers,
            ..Default::default()
        }
    }
}

const METRIC_ACCEPTED: &str = "perf_conn_accepted";
const METRIC_TERMINATED: &str = "perf_conn_terminated";
const METRIC_REJECTED: &str = "perf_conn_rejected";
const METRIC_ERROR: &str = "perf_conn_error";

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn load_bearing() {
    // ZG-PERFORMANCE-002
    //
    // The node sheds or rejects connections when necessary.
    //
    //  1. Start a node with max_peers set to `N`
    //  2. Initiate connections from `M > N` peer nodes
    //  3. Expect only `N` to be active at a time
    //
    // This test currently fails for both zebra and zcashd.
    //
    // ZCashd: Accepts only N-8 connections, possibly due to reserving
    //         connections for the hardcoded seeds (TBC).
    //
    // Zebra: Ignores the set limit, and does not appear to have an actual limit
    //        set. Start getting "address is in use" errors from M >= 15k.
    //
    // Example result:
    // *NOTE* run with `cargo test --release tests::performance::connections::load_bearing -- --nocapture`
    //
    // ZCashd:
    // ┌───────────┬───────┬────────────┬────────────┬────────────┬────────────┬────────────┬──────────┐
    // │           │       │ connection │ connection │ connection │ connection │ connection │          │
    // │ max peers │ peers │  accepted  │  rejected  │ terminated │    error   │  timed out │ time (s) │
    // ├───────────┼───────┼────────────┼────────────┼────────────┼────────────┼────────────┼──────────┤
    // │         50│    100│          42│          58│           0│           0│           0│      0.10│
    // ├───────────┼───────┼────────────┼────────────┼────────────┼────────────┼────────────┼──────────┤
    // │         50│   1000│          42│         958│           0│           0│           0│      0.14│
    // ├───────────┼───────┼────────────┼────────────┼────────────┼────────────┼────────────┼──────────┤
    // │         50│   5000│          42│        4958│           0│           0│           0│      0.27│
    // ├───────────┼───────┼────────────┼────────────┼────────────┼────────────┼────────────┼──────────┤
    // │         50│  10000│          42│        9958│           0│           0│           0│      1.30│
    // ├───────────┼───────┼────────────┼────────────┼────────────┼────────────┼────────────┼──────────┤
    // │         50│  15000│          42│       14958│           0│           0│           0│      1.36│
    // ├───────────┼───────┼────────────┼────────────┼────────────┼────────────┼────────────┼──────────┤
    // │         50│  20000│          42│       19958│           0│           0│           0│      1.60│
    // └───────────┴───────┴────────────┴────────────┴────────────┴────────────┴────────────┴──────────┘
    //
    // Zebra:
    // ┌───────────┬───────┬────────────┬────────────┬────────────┬────────────┬────────────┬──────────┐
    // │           │       │ connection │ connection │ connection │ connection │ connection │          │
    // │ max peers │ peers │  accepted  │  rejected  │ terminated │    error   │  timed out │ time (s) │
    // ├───────────┼───────┼────────────┼────────────┼────────────┼────────────┼────────────┼──────────┤
    // │         50│    100│         100│           0│           0│           0│           0│      0.20│
    // ├───────────┼───────┼────────────┼────────────┼────────────┼────────────┼────────────┼──────────┤
    // │         50│   1000│        1000│           0│           0│           0│           0│      0.72│
    // ├───────────┼───────┼────────────┼────────────┼────────────┼────────────┼────────────┼──────────┤
    // │         50│   5000│        4930│          70│           0│           0│           0│      3.57│
    // ├───────────┼───────┼────────────┼────────────┼────────────┼────────────┼────────────┼──────────┤
    // │         50│  10000│        8412│        1588│           0│           0│           0│      7.94│
    // ├───────────┼───────┼────────────┼────────────┼────────────┼────────────┼────────────┼──────────┤
    // │         50│  15000│       12282│         649│           0│         902│        1167│     20.20│
    // ├───────────┼───────┼────────────┼────────────┼────────────┼────────────┼────────────┼──────────┤
    // │         50│  20000│       10510│        1614│         400│        6800│        1076│     20.18│
    // └───────────┴───────┴────────────┴────────────┴────────────┴────────────┴────────────┴──────────┘
    //

    // setup metrics recorder
    simple_metrics::enable_simple_recorder().unwrap();

    // maximum time allowed for a single iteration of the test
    const MAX_ITER_TIME: Duration = Duration::from_secs(20);

    /// maximum peers to configure node with
    const MAX_PEERS: u16 = 50;

    let synth_counts = vec![100u16, 1_000, 5_000, 10_000, 15_000, 20_000];

    let mut all_stats = Vec::new();

    // start node
    let mut node = Node::new().unwrap();
    node.initial_action(Action::WaitForConnection)
        .max_peers(MAX_PEERS as usize)
        .start()
        .await
        .unwrap();

    for synth_count in synth_counts {
        // clear and register metrics
        simple_metrics::clear();
        metrics::register_counter!(METRIC_ACCEPTED);
        metrics::register_counter!(METRIC_TERMINATED);
        metrics::register_counter!(METRIC_REJECTED);
        metrics::register_counter!(METRIC_ERROR);

        let mut synth_handles = Vec::with_capacity(synth_count as usize);
        let mut synth_exits = Vec::with_capacity(synth_count as usize);
        let (handshake_tx, mut handshake_rx) =
            tokio::sync::mpsc::channel::<()>(synth_count as usize);

        let test_start = tokio::time::Instant::now();

        // start synthetic nodes
        for _ in 0..synth_count {
            let node_addr = node.addr();

            let (exit_tx, exit_rx) = tokio::sync::oneshot::channel::<()>();
            synth_exits.push(exit_tx);

            let synth_handshaken = handshake_tx.clone();
            // Synthetic node runs until it completes or is instructed to exit
            synth_handles.push(tokio::spawn(async move {
                tokio::select! {
                    _ = exit_rx => {},
                    _ = simulate_peer(node_addr, synth_handshaken) => {},
                };
            }));
        }

        // Wait for all peers to indicate that they've completed the handshake portion
        // or the iteration timeout is exceeded.
        let _ = tokio::time::timeout(MAX_ITER_TIME, async move {
            for _ in 0..synth_count {
                handshake_rx.recv().await.unwrap();
            }
        })
        .await;

        // Send stop signal to peer nodes. We ignore the possible error
        // result as this will occur with peers that have already exited.
        for stop in synth_exits {
            let _ = stop.send(());
        }

        // Wait for peers to complete
        for handle in synth_handles {
            handle.await.unwrap();
        }

        // Collect stats for this run
        let mut stats = Stats::new(MAX_PEERS, synth_count);
        stats.time = test_start.elapsed().as_secs_f64();
        {
            let counters = simple_metrics::counters();
            let counters_lock = counters.lock();
            stats.accepted = counters_lock
                .get(&metrics::Key::from_name(METRIC_ACCEPTED))
                .unwrap()
                .value as u16;
            stats.terminated = counters_lock
                .get(&metrics::Key::from_name(METRIC_TERMINATED))
                .unwrap()
                .value as u16;
            stats.rejected = counters_lock
                .get(&metrics::Key::from_name(METRIC_REJECTED))
                .unwrap()
                .value as u16;
            stats.conn_error = counters_lock
                .get(&metrics::Key::from_name(METRIC_ERROR))
                .unwrap()
                .value as u16;
            stats.timed_out = synth_count - stats.accepted - stats.rejected - stats.conn_error;
        }
        all_stats.push(stats);
    }

    node.stop().await.unwrap();

    // Display results table
    println!("{}", fmt_table(Table::new(&all_stats)));

    // Check that results are okay
    for stats in all_stats.iter() {
        // We currently assume no accepted peer connection gets terminated.
        // This can technically occur, but currently doesn't as all our peer
        // nodes are created equal (so the node doesn't drop our existing peers in
        // favor of new connections).
        //
        // If this is no longer true, then we need to start tracking the statistics
        // over time instead of just totals. So this is a sanity check to ensure that
        // assumption still applies.
        assert_eq!(stats.terminated, 0, "Stats: {:?}", stats);

        // We expect to have `MAX_PEERS` connections. This is only true if
        // `stats.terminated == 0`.
        assert_eq!(stats.accepted, MAX_PEERS, "Stats: {:?}", stats);

        // The rest of the peers should be rejected.
        assert_eq!(
            stats.rejected,
            stats.peers - MAX_PEERS,
            "Stats: {:?}",
            stats
        );

        // And no connection timeouts or errors
        assert_eq!(stats.timed_out, 0, "Stats: {:?}", stats);
        assert_eq!(stats.conn_error, 0, "Stats: {:?}", stats);
    }
}

async fn simulate_peer(node_addr: SocketAddr, handshake_complete: Sender<()>) {
    let mut synth_node = match SyntheticNode::builder()
        .with_full_handshake()
        .with_all_auto_reply()
        .build()
        .await
    {
        Ok(synth_node) => synth_node,
        Err(_) => {
            metrics::counter!(METRIC_ERROR, 1);
            return;
        }
    };

    // Establish peer connection
    let handshake_result = synth_node.connect(node_addr).await;
    handshake_complete.send(()).await.unwrap();
    match handshake_result {
        Ok(stream) => {
            metrics::counter!(METRIC_ACCEPTED, 1);
            stream
        }
        Err(_err) => {
            metrics::counter!(METRIC_REJECTED, 1);
            return;
        }
    };

    // Keep connection alive by replying to incoming Pings etc,
    // and check for terminated connection.
    //
    // We expect to receive no unfiltered messages.
    loop {
        match synth_node
            .recv_message_timeout(Duration::from_millis(100))
            .await
        {
            Ok((_, message)) => panic!("Unexpected message: {:?}", message),
            Err(_timeout) => {
                // check for broken connection
                if !synth_node.is_connected(node_addr) {
                    metrics::counter!(METRIC_TERMINATED, 1);
                    return;
                }
            }
        }
    }
}
