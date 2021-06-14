use crate::{
    helpers::{initiate_handshake, is_rejection_error, is_termination_error},
    protocol::message::{filter::MessageFilter, Message},
    setup::node::{Action, Node},
    tests::performance::table_float_display,
};

use tabled::{table, Alignment, Style, Tabled};

#[derive(Debug)]
enum PeerEvent {
    Rejected,
    Terminated,
    Connected,
    HandshakeError(std::io::Error),
    UnexpectedMessage(Box<Message>),
    ReadError(std::io::Error),
}

#[derive(Tabled, Default, Debug, Clone)]
struct Stats {
    #[header(" N ")]
    pub max_peers: u16,
    #[header(" M ")]
    pub peers: u16,
    #[header(" accepted ")]
    pub accepted: u16,
    #[header(" rejected ")]
    pub rejected: u16,
    #[header(" terminated ")]
    pub terminated: u16,
    #[header(" peak connections ")]
    pub peak_connected: u16,
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
    // ┌───┬─────┬──────────┬──────────┬────────────┬──────────────────┬──────────┐
    // │ N │  M  │ accepted │ rejected │ terminated │ peak connections │ time (s) │
    // ├───┼─────┼──────────┼──────────┼────────────┼──────────────────┼──────────┤
    // │ 50│  100│        42│        58│           0│                42│      0.05│
    // ├───┼─────┼──────────┼──────────┼────────────┼──────────────────┼──────────┤
    // │ 50│ 1000│        42│       958│           0│                42│      0.17│
    // ├───┼─────┼──────────┼──────────┼────────────┼──────────────────┼──────────┤
    // │ 50│ 5000│        42│      4958│           0│                42│      0.27│
    // ├───┼─────┼──────────┼──────────┼────────────┼──────────────────┼──────────┤
    // │ 50│10000│        42│      9958│           0│                42│      1.24│
    // ├───┼─────┼──────────┼──────────┼────────────┼──────────────────┼──────────┤
    // │ 50│15000│        42│     14958│           0│                42│      3.19│
    // ├───┼─────┼──────────┼──────────┼────────────┼──────────────────┼──────────┤
    // │ 50│20000│        42│     19958│           0│                42│      3.20│
    // └───┴─────┴──────────┴──────────┴────────────┴──────────────────┴──────────┘
    //
    // Zebra:
    // ┌───┬─────┬──────────┬──────────┬────────────┬──────────────────┬──────────┐
    // │ N │  M  │ accepted │ rejected │ terminated │ peak connections │ time (s) │
    // ├───┼─────┼──────────┼──────────┼────────────┼──────────────────┼──────────┤
    // │ 50│  100│       100│         0│           0│               100│      0.11│
    // ├───┼─────┼──────────┼──────────┼────────────┼──────────────────┼──────────┤
    // │ 50│ 1000│      1000│         0│           0│              1000│      0.66│
    // ├───┼─────┼──────────┼──────────┼────────────┼──────────────────┼──────────┤
    // │ 50│ 5000│      3785│      1215│           0│              3785│      2.27│
    // ├───┼─────┼──────────┼──────────┼────────────┼──────────────────┼──────────┤
    // │ 50│10000│      6146│      3854│           0│              6146│      7.58│
    // └───┴─────┴──────────┴──────────┴────────────┴──────────────────┴──────────┘
    //

    /// maximum peers to configure node with
    const MAX_PEERS: u16 = 50;

    let peer_counts = vec![100u16, 1_000, 5_000, 10_000]; //15_000, 20_000];

    let mut all_stats = Vec::new();

    // start node
    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection)
        .max_peers(MAX_PEERS as usize)
        .start()
        .await;

    for peers in peer_counts {
        let test_start = tokio::time::Instant::now();
        // channel for peer event management (ensure buffer is more than large enough)
        let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<PeerEvent>(peers as usize * 3);

        // start peer event manager
        let event_manager = tokio::spawn(async move {
            let mut stats = Stats::new(MAX_PEERS, peers);
            let mut active_connections = 0u16;

            loop {
                match event_rx.recv().await.unwrap() {
                    PeerEvent::Rejected => stats.rejected += 1,
                    PeerEvent::Terminated => {
                        stats.terminated += 1;
                        active_connections -= 1;
                    }
                    PeerEvent::Connected => {
                        stats.accepted += 1;
                        active_connections += 1;
                        stats.peak_connected = stats.peak_connected.max(active_connections);
                    }
                    PeerEvent::HandshakeError(err) => {
                        panic!("{} - Handshake error: {:?}", peers, err)
                    }
                    PeerEvent::UnexpectedMessage(msg) => {
                        panic!("{} - Unexpected message: {:?}", peers, msg)
                    }
                    PeerEvent::ReadError(err) => panic!("{} - Read error:\n{:?}", peers, err),
                }

                // We are done if all peer connections have either been accepted or rejected
                if stats.accepted + stats.rejected == peers {
                    break;
                }
            }
            stats
        });

        // start peer nodes
        let mut peer_handles = Vec::with_capacity(peers as usize);
        let mut peer_exits = Vec::with_capacity(peers as usize);

        for _ in 0..peers {
            let node_addr = node.addr();
            let peer_send = event_tx.clone();

            let (exit_tx, exit_rx) = tokio::sync::oneshot::channel::<()>();
            peer_exits.push(exit_tx);

            peer_handles.push(tokio::spawn(async move {
                // Establish peer connection
                let mut stream = match initiate_handshake(node_addr).await {
                    Ok(stream) => {
                        let _ = peer_send.send(PeerEvent::Connected).await;
                        stream
                    }
                    Err(err) if is_rejection_error(&err) => {
                        let _ = peer_send.send(PeerEvent::Rejected).await;
                        return;
                    }
                    Err(err) => {
                        let _ = peer_send.send(PeerEvent::HandshakeError(err)).await;
                        return;
                    }
                };

                // Keep connection alive by replying to incoming Pings etc, until instructed to exit or
                // connection is terminated (or something unexpected occurs).
                let filter = MessageFilter::with_all_auto_reply();
                tokio::select! {
                    _ = exit_rx => {},
                    result = filter.read_from_stream(&mut stream) => {
                        match result {
                            Ok(message) => {
                                let _ = peer_send
                                    .send(PeerEvent::UnexpectedMessage(message.into()))
                                    .await;
                            }
                            Err(err) if is_termination_error(&err) => {
                                let _ = peer_send.send(PeerEvent::Terminated).await;
                            }
                            Err(err) => {
                                let _ = peer_send.send(PeerEvent::ReadError(err)).await;
                            }
                        }
                    }
                }
            }));
        }

        // Wait for event manager to complete its tally
        let mut stats = event_manager.await.unwrap();
        stats.time = test_start.elapsed().as_secs_f64();
        all_stats.push(stats);

        // Send stop signal to peer nodes. We ignore the possible error
        // result as this will occur with peers that have already exited.
        for stop in peer_exits {
            let _ = stop.send(());
        }

        // Wait for peers to complete
        for handle in peer_handles {
            handle.await.unwrap();
        }
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
