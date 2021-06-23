use rand::{prelude::SliceRandom, Rng};
use rand_chacha::ChaCha8Rng;
use std::{net::SocketAddr, time::Duration};
use tabled::{table, Alignment, Style, Tabled};

use crate::{
    tools::synthetic_node::SyntheticNode,
    protocol::{
        message::{constants::MAGIC, Message, MessageHeader},
        payload::{
            block::{Block, Headers, LocatorHashes},
            codec::Codec,
            Hash, Inv, Nonce,
        },
    },
    setup::node::{Action, Node},
    tests::{
        performance::{duration_as_ms, table_float_display, RequestStats, RequestsTable},
        resistance::{
            default_fuzz_messages,
            fuzzing_corrupted_messages::slightly_corrupted_messages,
            fuzzing_incorrect_checksum::encode_messages_and_corrupt_checksum,
            fuzzing_incorrect_length::encode_messages_and_corrupt_body_length_field,
            fuzzing_random_bytes::{metadata_compliant_random_bytes, random_bytes},
            fuzzing_zeroes::zeroes,
            seeded_rng, COMMANDS_WITH_PAYLOADS,
        },
        simple_metrics,
    },
};

#[derive(Default, Tabled)]
struct Stats {
    peers: usize,
    requests: usize,
    #[header(" handshakes \n accepted ")]
    handshake_accepted: u16,
    #[header(" handshakes \n rejected ")]
    handshake_rejected: u16,
    #[header(" peers \n dropped ")]
    peers_dropped: u16,
    #[header(" corrupt \n terminated ")]
    corrupt_terminated: u16,
    #[header(" corrupt \n rejected ")]
    corrupt_rejected: u16,
    #[header(" corrupt \n replied ")]
    corrupt_reply: u16,
    #[header(" corrupt \n ignored ")]
    corrupt_ignored: u16,
    #[header(" bad \n replies ")]
    reply_errors: u16,

    #[header(" hung \n connections ")]
    dangling: u16,
    #[header(" time (s) ")]
    #[field(display_with = "table_float_display")]
    time: f64,
}

const REQUEST_LATENCY: &str = "fuzz_flood_request_latency";
const HANDSHAKE_LATENCY: &str = "fuzz_flood_handshake_latency";

const HANDSHAKE_ACCEPTED: &str = "fuzz_flood_handshake_accepted";
const HANDSHAKE_REJECTED: &str = "fuzz_flood_handshake_rejected";

const CONNECTION_TERMINATED: &str = "fuzz_flood_connection_terminated";
const BAD_REPLY: &str = "fuzz_flood_bad_reply";

const CORRUPT_TERMINATED: &str = "fuzz_flood_corrupt_terminated";
const CORRUPT_REJECTED: &str = "fuzz_flood_corrupt_rejected";
const CORRUPT_REPLY: &str = "fuzz_flood_corrupt_reply";
const CORRUPT_IGNORED: &str = "fuzz_flood_corrupt_ignored";

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
    // ┌─────┬────────┬────────────┬────────────┬─────────┬────────────┬──────────┬─────────┬─────────┬─────────┬─────────────┬──────────┐
    // │     │        │ handshakes │ handshakes │  peers  │   corrupt  │  corrupt │ corrupt │ corrupt │   bad   │     hung    │          │
    // │peers│requests│  accepted  │  rejected  │ dropped │ terminated │ rejected │ replied │ ignored │ replies │ connections │ time (s) │
    // ├─────┼────────┼────────────┼────────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼─────────────┼──────────┤
    // │    1│     100│           1│           0│        0│           0│         0│        0│        1│        0│            0│      0.06│
    // ├─────┼────────┼────────────┼────────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼─────────────┼──────────┤
    // │   10│     100│          10│           0│        0│           8│         0│        0│        0│        0│            2│     13.12│
    // ├─────┼────────┼────────────┼────────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼─────────────┼──────────┤
    // │   20│     100│          20│           0│        0│          17│         0│        0│        2│        0│            1│     25.90│
    // ├─────┼────────┼────────────┼────────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼─────────────┼──────────┤
    // │   30│     100│          30│           0│        0│          22│         3│        0│        3│        0│            2│     38.65│
    // ├─────┼────────┼────────────┼────────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼─────────────┼──────────┤
    // │   40│     100│          40│           0│        0│          28│         5│        0│        7│        0│            0│     41.60│
    // ├─────┼────────┼────────────┼────────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼─────────────┼──────────┤
    // │   50│     100│          50│           0│        0│          38│         1│        0│        6│        0│            5│     54.86│
    // ├─────┼────────┼────────────┼────────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼─────────────┼──────────┤
    // │   60│     100│          60│           0│        0│          45│         4│        0│        6│        0│            5│     68.21│
    // ├─────┼────────┼────────────┼────────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼─────────────┼──────────┤
    // │   70│     100│          70│           0│        0│          52│         2│        0│       13│        0│            3│     81.38│
    // ├─────┼────────┼────────────┼────────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼─────────────┼──────────┤
    // │   80│     100│          80│           0│        0│          69│         3│        0│        5│        0│            3│     94.56│
    // ├─────┼────────┼────────────┼────────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼─────────────┼──────────┤
    // │   90│     100│          90│           0│        0│          61│         4│        0│       17│        0│            8│    107.70│
    // ├─────┼────────┼────────────┼────────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼─────────────┼──────────┤
    // │  100│     100│         100│           0│        0│          72│         6│        0│       15│        0│            7│    120.94│
    // ├─────┼────────┼────────────┼────────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼─────────────┼──────────┤
    // │  200│     100│         200│           0│        0│         157│         7│        0│       25│        0│           11│    134.95│
    // ├─────┼────────┼────────────┼────────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼─────────────┼──────────┤
    // │  300│     100│         300│           0│        0│         243│        12│        0│       35│        0│           10│    149.97│
    // ├─────┼────────┼────────────┼────────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼─────────────┼──────────┤
    // │  500│     100│         500│           0│        0│         374│        32│        0│       70│        0│           24│    166.31│
    // ├─────┼────────┼────────────┼────────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼─────────────┼──────────┤
    // │  750│     100│         750│           0│        0│         554│        52│        0│      102│        0│           42│    184.74│
    // ├─────┼────────┼────────────┼────────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼─────────────┼──────────┤
    // │  800│     100│         800│           0│        0│         630│        46│        0│       93│        0│           31│    203.76│
    // └─────┴────────┴────────────┴────────────┴─────────┴────────────┴──────────┴─────────┴─────────┴─────────┴─────────────┴──────────┘
    //
    // Request latencies
    // ┌───────┬──────────┬──────────┬──────────┬──────────────┬──────────┬──────────┬──────────┬──────────┬──────────┬──────────┬────────────┐
    // │ peers │ requests │ min (ms) │ max (ms) │ std dev (ms) │ 10% (ms) │ 50% (ms) │ 75% (ms) │ 90% (ms) │ 99% (ms) │ time (s) │ requests/s │
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │      1│       100│         0│         0│             0│         0│         0│         0│         0│         0│      0.06│     1602.86│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     10│       100│         0│       101│            20│         0│         0│         1│        51│       100│     13.12│       76.25│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     20│       100│         0│        55│            17│         0│         1│         2│        51│        53│     25.90│       77.21│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     30│       100│         0│        55│            15│         1│         2│         3│         6│        53│     38.65│       77.62│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     40│       100│         0│       103│            18│         1│         4│         4│        45│        56│     41.60│       96.14│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     50│       100│         0│       101│            17│         1│         4│         6│        51│        55│     54.86│       91.14│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     60│       100│         0│       101│            19│         2│         6│         8│        56│        59│     68.21│       87.97│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     70│       100│         0│       104│            16│         2│         6│         8│        45│        58│     81.38│       86.02│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     80│       100│         0│       106│            18│         3│         7│         9│        31│       104│     94.56│       84.60│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     90│       100│         0│        70│            15│         3│         8│        10│        18│        61│    107.70│       83.57│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │    100│       100│         0│        65│            14│         5│         9│        12│        21│        63│    120.94│       82.69│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │    200│       100│         0│        86│            17│         7│        20│        25│        55│        75│    134.95│      148.21│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │    300│       100│         0│       127│            17│        13│        29│        35│        54│        90│    149.97│      200.04│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │    500│       100│         0│       243│            25│        19│        47│        60│        79│       147│    166.31│      300.64│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │    750│       100│         0│       194│            25│        32│        65│        79│        87│       148│    184.74│      405.98│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │    800│       100│         0│       215│            26│        31│        69│        80│        91│       151│    203.76│      392.62│
    // └───────┴──────────┴──────────┴──────────┴──────────────┴──────────┴──────────┴──────────┴──────────┴──────────┴──────────┴────────────┘
    //
    // Handshake latencies
    // ┌───────┬──────────┬──────────┬──────────┬──────────────┬──────────┬──────────┬──────────┬──────────┬──────────┬──────────┬────────────┐
    // │ peers │ requests │ min (ms) │ max (ms) │ std dev (ms) │ 10% (ms) │ 50% (ms) │ 75% (ms) │ 90% (ms) │ 99% (ms) │ time (s) │ requests/s │
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │      1│         1│         0│         0│             0│         0│         0│         0│         0│         0│      0.06│       16.03│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     10│         1│         1│         4│             2│         1│         2│         4│         4│         4│     13.12│        0.76│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     20│         1│         0│         7│             3│         1│         2│         4│         6│         7│     25.90│        0.77│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     30│         1│         0│       113│            29│         1│         5│         8│        58│       113│     38.65│        0.78│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     40│         1│         0│        59│            17│         1│         7│         9│        59│        59│     41.60│        0.96│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     50│         1│         0│        15│             5│         1│         8│        11│        14│        15│     54.86│        0.91│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     60│         1│         0│       116│            20│         1│        10│        15│        20│       116│     68.21│        0.88│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     70│         1│         0│        32│            10│         1│        15│        20│        25│        32│     81.38│        0.86│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     80│         1│         0│        30│             9│         2│        11│        22│        30│        30│     94.56│        0.85│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     90│         1│         0│        37│            12│         2│        14│        28│        36│        37│    107.70│        0.84│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │    100│         1│         0│        54│            16│         2│        17│        31│        43│        54│    120.94│        0.83│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │    200│         1│         0│        84│            21│        26│        63│        65│        68│        84│    134.95│        1.48│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │    300│         1│         0│       385│           103│        51│        63│       111│       315│       351│    149.97│        2.00│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │    500│         1│         0│       528│           145│        23│       111│       208│       450│       527│    166.31│        3.01│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │    750│         1│         0│      1345│           400│        83│       416│       764│      1070│      1344│    184.74│        4.06│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │    800│         1│         1│      1656│           486│        45│       331│       903│      1371│      1558│    203.76│        3.93│
    // └───────┴──────────┴──────────┴──────────┴──────────────┴──────────┴──────────┴──────────┴──────────┴──────────┴──────────┴────────────┘

    // enable simple metrics recording
    simple_metrics::enable_simple_recorder().unwrap();

    const TIMEOUT: tokio::time::Duration = tokio::time::Duration::from_secs(10);

    let mut request_table = RequestsTable::default();
    let mut handshake_table = RequestsTable::default();
    let mut stats = Vec::<Stats>::new();

    // Create a pool of valid and invalid message types
    const MAX_VALID_MESSAGES: usize = 100;
    let synth_counts = vec![
        1, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 200, 300, 500, 750, 800,
    ];

    let mut rng = seeded_rng();
    let valid_pool = valid_queries_responses();

    // Start node with arbitrarily higher max peer count than what we
    // need for the test. Note that zcashd node appears to reserver 8
    // slots (hence the +10).
    let mut node: Node = Default::default();
    node.initial_action(Action::SeedWithTestnetBlocks(3))
        .max_peers(synth_counts.iter().max().unwrap() * 2 + 10)
        .start()
        .await;

    let node_addr = node.addr();

    let iteration_timer = tokio::time::Instant::now();

    // Iterate over peer counts
    // (note: rng usage should stay in a single thread in order for it be somewhat repeatable - we can't account
    // for relative timings and state transitions in the node).
    for peers in synth_counts {
        // register metrics
        simple_metrics::clear();
        metrics::register_histogram!(REQUEST_LATENCY);
        metrics::register_histogram!(HANDSHAKE_LATENCY);
        metrics::register_counter!(HANDSHAKE_ACCEPTED);
        metrics::register_counter!(HANDSHAKE_REJECTED);
        metrics::register_counter!(CONNECTION_TERMINATED);
        metrics::register_counter!(BAD_REPLY);
        metrics::register_counter!(CORRUPT_TERMINATED);
        metrics::register_counter!(CORRUPT_REJECTED);
        metrics::register_counter!(CORRUPT_REPLY);
        metrics::register_counter!(CORRUPT_IGNORED);

        // Generate the broken fuzz messages for this peer set (one per peer, since this should break the connection)
        let mut corrupt_messages = generate_corrupt_messages(&mut rng, peers);

        let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(peers);

        // Start the N peers for this iteration
        let mut synth_handles = Vec::with_capacity(peers);
        let mut synth_exits = Vec::with_capacity(peers);
        for _ in 0..peers {
            let (exit_tx, exit_rx) = tokio::sync::oneshot::channel::<()>();
            synth_exits.push(exit_tx);

            let synth_complete_notifier = tx.clone();

            // generate the valid message set for this peer
            let valid = (0..rng.gen_range(0..MAX_VALID_MESSAGES))
                .map(|_| valid_pool.choose(&mut rng).unwrap().clone())
                .collect::<Vec<_>>();
            // grab the broken message for this peer
            let corrupt = corrupt_messages.pop().unwrap();

            // let peer_tx = event_tx.clone();

            // start and wait for peer simulation to complete, or cancel if exit signal is received
            synth_handles.push(tokio::spawn(async move {
                tokio::select! {
                    _ = exit_rx => {},
                    _ = simulate_peer(node_addr, valid, corrupt) => {},
                }
                synth_complete_notifier.send(()).await.unwrap();
            }));
        }

        // wait for each peer to indicate completion via the channel, or timeout (indicating that remaining peers are stuck)
        let mut completed = 0;
        for _ in 0..peers {
            match tokio::time::timeout(TIMEOUT, rx.recv()).await {
                Ok(_result) => completed += 1,
                Err(_timeout) => break,
            }
        }

        // instruct any hanging peers to exit
        for exit in synth_exits {
            let _ = exit.send(());
        }

        // Wait for peers to complete
        for handle in synth_handles {
            handle.await.unwrap();
        }
        let iteration_time = iteration_timer.elapsed().as_secs_f64();

        // update request latencies table
        let request_latencies = simple_metrics::histograms()
            .lock()
            .get(&metrics::Key::from_name(REQUEST_LATENCY))
            .unwrap()
            .value
            .clone();
        let row = RequestStats::new(
            peers as u16,
            MAX_VALID_MESSAGES as u16,
            request_latencies,
            iteration_time,
        );
        request_table.add_row(row);

        // update handshake latencies table
        let handshake_latencies = simple_metrics::histograms()
            .lock()
            .get(&metrics::Key::from_name(HANDSHAKE_LATENCY))
            .unwrap()
            .value
            .clone();
        let row = RequestStats::new(peers as u16, 1, handshake_latencies, iteration_time);
        handshake_table.add_row(row);

        // update stats table
        let mut stat = Stats {
            peers,
            requests: MAX_VALID_MESSAGES,
            time: iteration_time,
            dangling: peers as u16 - completed,
            ..Default::default()
        };
        {
            let counters = simple_metrics::counters();
            let locked_counters = counters.lock();
            stat.handshake_accepted = locked_counters
                .get(&metrics::Key::from_name(HANDSHAKE_ACCEPTED))
                .unwrap()
                .value as u16;
            stat.handshake_rejected = locked_counters
                .get(&metrics::Key::from_name(HANDSHAKE_REJECTED))
                .unwrap()
                .value as u16;

            stat.corrupt_terminated = locked_counters
                .get(&metrics::Key::from_name(CORRUPT_TERMINATED))
                .unwrap()
                .value as u16;
            stat.corrupt_ignored = locked_counters
                .get(&metrics::Key::from_name(CORRUPT_IGNORED))
                .unwrap()
                .value as u16;
            stat.corrupt_rejected = locked_counters
                .get(&metrics::Key::from_name(CORRUPT_REJECTED))
                .unwrap()
                .value as u16;
            stat.corrupt_reply = locked_counters
                .get(&metrics::Key::from_name(CORRUPT_REPLY))
                .unwrap()
                .value as u16;

            stat.peers_dropped = locked_counters
                .get(&metrics::Key::from_name(CONNECTION_TERMINATED))
                .unwrap()
                .value as u16;
            stat.reply_errors = locked_counters
                .get(&metrics::Key::from_name(BAD_REPLY))
                .unwrap()
                .value as u16;
        }

        stats.push(stat);
    }

    // Display tables
    println!(
        "Stats\n{}\n",
        table!(
            stats,
            Style::pseudo(),
            Alignment::center_vertical(tabled::Full),
            Alignment::right(tabled::Column(..)),
            Alignment::center_horizontal(tabled::Head),
        )
    );
    println!("Request latencies\n{}\n", request_table);
    println!("Handshake latencies\n{}\n", handshake_table);

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
    possible_payloads.append(&mut metadata_compliant_random_bytes(
        rng,
        n,
        &COMMANDS_WITH_PAYLOADS,
    ));

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
    message_pairs: Vec<(Message, Message)>,
    corrupt_message: Vec<u8>,
) {
    const READ_TIMEOUT: Duration = Duration::from_secs(2);

    let mut synth_node = SyntheticNode::builder()
        .with_all_auto_reply()
        .with_full_handshake()
        .with_max_write_buffer_size(std::cmp::max(corrupt_message.len(), 65536))
        .build()
        .await
        .unwrap();

    // handshake
    let timer = tokio::time::Instant::now();
    let handshake_result = synth_node.connect(node_addr).await;
    metrics::histogram!(HANDSHAKE_LATENCY, duration_as_ms(timer.elapsed()));
    match handshake_result {
        Ok(_) => metrics::counter!(HANDSHAKE_ACCEPTED, 1),
        Err(_) => metrics::counter!(HANDSHAKE_REJECTED, 1),
    }

    // send the valid messages and validate the response
    for (msg, expected) in message_pairs {
        if synth_node
            .send_direct_message(node_addr, msg)
            .await
            .is_err()
        {
            metrics::counter!(CONNECTION_TERMINATED, 1);
            return;
        }

        let timer = tokio::time::Instant::now();

        match synth_node.recv_message_timeout(READ_TIMEOUT).await {
            Ok((_, reply)) if reply == expected => {
                metrics::histogram!(REQUEST_LATENCY, duration_as_ms(timer.elapsed()));
            }
            Ok((_, reply)) => {
                metrics::counter!(BAD_REPLY, 1);
                panic!("Bad reply received: {:?}", reply);
            }
            Err(_timeout) => {
                // recv timed out, check if connection was terminated or if its just a slow reply
                if synth_node.is_connected(node_addr) {
                    metrics::histogram!(REQUEST_LATENCY, READ_TIMEOUT.as_millis() as f64);
                } else {
                    metrics::counter!(CONNECTION_TERMINATED, 1);
                    return;
                }
            }
        }
    }

    // send the corrupt message
    if synth_node
        .send_direct_bytes(node_addr, corrupt_message)
        .await
        .is_err()
    {
        metrics::counter!(CONNECTION_TERMINATED, 1);
        return;
    }

    //  check for termination by sending a ping -> pong (should result in a terminated connection prior to the pong)
    let nonce = Nonce::default();
    if synth_node
        .send_direct_message(node_addr, Message::Ping(nonce))
        .await
        .is_err()
    {
        metrics::counter!(CORRUPT_TERMINATED, 1);
        return;
    }

    // loop so we can check if connection has been terminated inbetween waiting on reads
    let read_result = loop {
        let result = synth_node.recv_message_timeout(READ_TIMEOUT).await;
        // We break out if we either
        //  1. received a reply
        //  2. the connection was terminated
        if result.is_ok() || !synth_node.is_connected(node_addr) {
            break result;
        }
    };

    match read_result {
        Ok((_, Message::Pong(rx_nonce))) if nonce == rx_nonce => {
            metrics::counter!(CORRUPT_IGNORED, 1);
        }
        Ok((_, Message::Reject(..))) => {
            metrics::counter!(CORRUPT_REJECTED, 1);
        }
        Ok((_, message)) => {
            metrics::counter!(CORRUPT_REPLY, 1);
            panic!("Reply received to corrupt message: {:?}", message);
        }
        Err(_timeout) => {
            metrics::counter!(CORRUPT_TERMINATED, 1);
        }
    }
}
