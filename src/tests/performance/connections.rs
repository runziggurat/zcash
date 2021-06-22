use crate::{
    helpers::synthetic_peers::SyntheticNode,
    setup::node::{Action, Node},
    tests::{performance::table_float_display, simple_metrics},
};

use tabled::{table, Alignment, Style, Tabled};

#[derive(Tabled, Default, Debug, Clone)]
struct Stats {
    #[header(" max peers ")]
    pub max_peers: u16,
    #[header(" peers ")]
    pub peers: u16,
    #[header(" accepted ")]
    pub accepted: u16,
    #[header(" rejected ")]
    pub rejected: u16,
    #[header(" terminated ")]
    pub terminated: u16,
    #[header(" time (s) ")]
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

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn incoming_active_connections() {
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
    //        set. Handshakes start timing out around M=15k.
    //
    // Example result:
    // *NOTE* run with `cargo test --release tests::performance::connections::incoming_active_connections -- --nocapture`
    //
    // ZCashd:
    // ┌───────────┬───────┬──────────┬──────────┬────────────┬──────────┐
    // │ max peers │ peers │ accepted │ rejected │ terminated │ time (s) │
    // ├───────────┼───────┼──────────┼──────────┼────────────┼──────────┤
    // │         50│    100│        42│        58│           0│      0.08│
    // ├───────────┼───────┼──────────┼──────────┼────────────┼──────────┤
    // │         50│   1000│        42│       958│           0│      0.10│
    // ├───────────┼───────┼──────────┼──────────┼────────────┼──────────┤
    // │         50│   5000│        42│      4958│           0│      0.32│
    // ├───────────┼───────┼──────────┼──────────┼────────────┼──────────┤
    // │         50│  10000│        42│      9958│           0│      0.60│
    // ├───────────┼───────┼──────────┼──────────┼────────────┼──────────┤
    // │         50│  15000│        42│     14958│           0│      1.48│
    // ├───────────┼───────┼──────────┼──────────┼────────────┼──────────┤
    // │         50│  20000│        42│     19958│           0│      1.59│
    // └───────────┴───────┴──────────┴──────────┴────────────┴──────────┘
    //
    // Zebra:
    // ┌───────────┬───────┬──────────┬──────────┬────────────┬──────────┐
    // │ max peers │ peers │ accepted │ rejected │ terminated │ time (s) │
    // ├───────────┼───────┼──────────┼──────────┼────────────┼──────────┤
    // │         50│    100│       100│         0│           0│      0.13│
    // ├───────────┼───────┼──────────┼──────────┼────────────┼──────────┤
    // │         50│   1000│      1000│         0│           0│      0.82│
    // ├───────────┼───────┼──────────┼──────────┼────────────┼──────────┤
    // │         50│   5000│      4564│       436│           0│      3.64│
    // ├───────────┼───────┼──────────┼──────────┼────────────┼──────────┤
    // │         50│  10000│      6909│      3091│           0│      7.36│
    // └───────────┴───────┴──────────┴──────────┴────────────┴──────────┘
    //

    // setup metrics recorder
    simple_metrics::enable_simple_recorder().unwrap();

    /// maximum peers to configure node with
    const MAX_PEERS: u16 = 50;

    const METRIC_ACCEPTED: &str = "perf_conn_accepted";
    const METRIC_TERMINATED: &str = "perf_conn_terminated";
    const METRIC_REJECTED: &str = "perf_conn_rejected";

    let peer_counts = vec![100u16, 1_000, 5_000, 10_000, 15_000, 20_000];

    let mut all_stats = Vec::new();

    // start node
    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection)
        .max_peers(MAX_PEERS as usize)
        .start()
        .await;

    for peers in peer_counts {
        // clear and register metrics
        simple_metrics::clear();
        metrics::register_counter!(METRIC_ACCEPTED);
        metrics::register_counter!(METRIC_TERMINATED);
        metrics::register_counter!(METRIC_REJECTED);

        let test_start = tokio::time::Instant::now();

        // start peer nodes
        let mut peer_handles = Vec::with_capacity(peers as usize);
        let mut peer_exits = Vec::with_capacity(peers as usize);
        let (handshake_tx, mut handshake_rx) = tokio::sync::mpsc::channel::<()>(peers as usize);

        for _ in 0..peers {
            let node_addr = node.addr();

            let (exit_tx, exit_rx) = tokio::sync::oneshot::channel::<()>();
            peer_exits.push(exit_tx);

            let peer_handshaken = handshake_tx.clone();

            peer_handles.push(tokio::spawn(async move {
                let mut peer = SyntheticNode::builder()
                    .with_full_handshake()
                    .with_all_auto_reply()
                    .build()
                    .await
                    .unwrap();

                // Establish peer connection
                let handshake_result = peer.connect(node_addr).await;
                peer_handshaken.send(()).await.unwrap();
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

                // Keep connection alive by replying to incoming Pings etc, until instructed to exit or
                // connection is terminated (or something unexpected occurs).
                tokio::select! {
                    _ = exit_rx => {},
                    (_, message) = peer.recv_message() => {
                        panic!("Unexpected message: {:?}", message);
                    }
                }
            }));
        }

        // Wait for all peers to indicate that they've completed the handshake portion
        for _ in 0..peers {
            handshake_rx.recv().await.unwrap();
        }

        // Send stop signal to peer nodes. We ignore the possible error
        // result as this will occur with peers that have already exited.
        for stop in peer_exits {
            let _ = stop.send(());
        }

        // Wait for peers to complete
        for handle in peer_handles {
            handle.await.unwrap();
        }

        // Collect stats for this run
        let mut stats = Stats::new(MAX_PEERS, peers);
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
        }
        all_stats.push(stats);
    }

    node.stop().await;

    // Display results table
    println!(
        "{}",
        table!(
            all_stats.clone(),
            Style::pseudo(),
            Alignment::center_vertical(tabled::Full),
            Alignment::right(tabled::Column(..)),
            Alignment::center_horizontal(tabled::Head),
        )
    );

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
    }
}
