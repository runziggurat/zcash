use rand::prelude::SliceRandom;
use rand_chacha::ChaCha8Rng;
use std::{net::SocketAddr, time::Duration};
use tabled::{Table, Tabled};

use crate::{
    protocol::{
        message::{constants::MAGIC, Message, MessageHeader},
        payload::{
            block::{Block, Headers, LocatorHashes},
            codec::Codec,
            Hash, Inv, Nonce,
        },
    },
    setup::node::{Action, Node},
    tests::resistance::{
        default_fuzz_messages,
        fuzzing_corrupted_messages::slightly_corrupted_messages,
        fuzzing_incorrect_checksum::encode_messages_and_corrupt_checksum,
        fuzzing_incorrect_length::encode_messages_and_corrupt_body_length_field,
        fuzzing_random_bytes::{metadata_compliant_random_bytes, random_bytes},
        seeded_rng,
        zeroes::zeroes,
        COMMANDS_WITH_PAYLOADS,
    },
    tools::{
        metrics::{
            recorder,
            tables::{duration_as_ms, fmt_table, table_float_display, RequestStats, RequestsTable},
        },
        synthetic_node::SyntheticNode,
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
async fn throughput() {
    // ZG-RESISTANCE-006
    //
    // Simulate high load situations, while also sending corrupt messages.
    //
    // We simulate active peers which send 100 requests (and validate the replies), and finally send a corrupt message
    // which ends the connection. We measure the latency of the replies and the type of response to the corrupt message.
    //
    //  *NOTE* run with `cargo test --release tests::resistance::fuzzing_flood::throughput -- --nocapture`
    //
    // Currently only works for zcashd as zebra does not support block seeding.
    //
    // ZCashd: Terminates most of the time, but also rejects and ignores some of the messages. In some instances the connection hangs,
    //         all instances investigated were as a result of a corrupt payload length leading to the node waiting for more data.
    //         This is likely the desired and correct behaviour.
    //
    // Stats
    // ┌─────┬────────┬────────────┬────────────┬─────────┬────────────┬──────────┬─────────┬─────────┬─────────┬─────────────┬──────────┐
    // │peers│requests│ handshakes │ handshakes │  peers  │  corrupt   │ corrupt  │ corrupt │ corrupt │   bad   │    hung     │ time (s) │
    // │     │        │  accepted  │  rejected  │ dropped │ terminated │ rejected │ replied │ ignored │ replies │ connections │          │
    // ├─────┼────────┼────────────┼────────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼─────────────┼──────────┤
    // │    1│     100│           1│           0│        0│           1│         0│        0│        0│        0│            0│      2.03│
    // ├─────┼────────┼────────────┼────────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼─────────────┼──────────┤
    // │   10│     100│          10│           0│        0│           8│         0│        0│        1│        0│            1│     22.90│
    // ├─────┼────────┼────────────┼────────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼─────────────┼──────────┤
    // │   20│     100│          20│           0│        0│          17│         2│        0│        1│        0│            0│      3.22│
    // ├─────┼────────┼────────────┼────────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼─────────────┼──────────┤
    // │   30│     100│          30│           0│        0│          18│         2│        0│        9│        0│            1│     23.62│
    // ├─────┼────────┼────────────┼────────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼─────────────┼──────────┤
    // │   40│     100│          40│           0│        0│          32│         1│        0│        6│        0│            1│     23.47│
    // ├─────┼────────┼────────────┼────────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼─────────────┼──────────┤
    // │   50│     100│          50│           0│        0│          39│         2│        0│        9│        0│            0│      5.94│
    // ├─────┼────────┼────────────┼────────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼─────────────┼──────────┤
    // │   60│     100│          60│           0│        0│          44│         1│        0│       12│        0│            3│     23.50│
    // ├─────┼────────┼────────────┼────────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼─────────────┼──────────┤
    // │   70│     100│          70│           0│        0│          52│         6│        0│        8│        0│            4│     24.20│
    // ├─────┼────────┼────────────┼────────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼─────────────┼──────────┤
    // │   80│     100│          80│           0│        0│          64│         3│        0│       10│        0│            3│     24.09│
    // ├─────┼────────┼────────────┼────────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼─────────────┼──────────┤
    // │   90│     100│          90│           0│        0│          67│         7│        0│        8│        0│            8│     24.09│
    // ├─────┼────────┼────────────┼────────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼─────────────┼──────────┤
    // │  100│     100│         100│           0│        0│          70│         8│        0│       15│        0│            7│     24.24│
    // ├─────┼────────┼────────────┼────────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼─────────────┼──────────┤
    // │  200│     100│         200│           0│        0│         156│        11│        0│       22│        0│           11│     25.57│
    // ├─────┼────────┼────────────┼────────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼─────────────┼──────────┤
    // │  300│     100│         300│           0│        0│         225│        14│        0│       48│        0│           13│     26.81│
    // ├─────┼────────┼────────────┼────────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼─────────────┼──────────┤
    // │  500│     100│         500│           0│        0│         400│        26│        0│       49│        0│           25│     29.64│
    // ├─────┼────────┼────────────┼────────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼─────────────┼──────────┤
    // │  750│     100│         750│           0│        0│         560│        52│        0│       93│        0│           45│     33.11│
    // ├─────┼────────┼────────────┼────────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼─────────────┼──────────┤
    // │  800│     100│         800│           0│        0│         593│        53│        0│      108│        0│           46│     34.23│
    // └─────┴────────┴────────────┴────────────┴─────────┴────────────┴──────────┴─────────┴─────────┴─────────┴─────────────┴──────────┘
    //
    // Request latencies
    // ┌───────┬──────────┬──────────┬──────────┬──────────────┬──────────┬──────────┬──────────┬──────────┬──────────┬──────────────┬──────────┬────────────┐
    // │ peers │ requests │ min (ms) │ max (ms) │ std dev (ms) │ 10% (ms) │ 50% (ms) │ 75% (ms) │ 90% (ms) │ 99% (ms) │ completion % │ time (s) │ requests/s │
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │      1│       100│         0│         0│             0│         0│         0│         0│         0│         0│        100.00│      2.03│       49.30│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │     10│       100│         0│        52│            18│         0│         1│         1│        51│        52│        100.00│     22.90│       43.68│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │     20│       100│         0│        54│            20│         1│         2│         3│        52│        54│        100.00│      3.22│      620.29│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │     30│       100│         0│        57│            21│         2│         4│         5│        54│        55│        100.00│     23.62│      126.99│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │     40│       100│         0│        60│            20│         3│         5│         6│        55│        56│        100.00│     23.47│      170.46│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │     50│       100│         1│      1896│            39│         5│         6│         7│        56│        57│        100.00│      5.94│      841.43│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │     60│       100│         0│        60│            17│         6│         8│         9│        56│        59│        100.00│     23.50│      255.35│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │     70│       100│         0│        67│            22│         7│         9│        11│        59│        60│        100.00│     24.20│      289.23│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │     80│       100│         0│        73│            20│         8│        10│        12│        59│        61│        100.00│     24.09│      332.05│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │     90│       100│         0│        73│            19│         9│        11│        13│        61│        63│        100.00│     24.09│      373.58│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │    100│       100│         0│       124│            19│        10│        12│        14│        62│        64│        100.00│     24.24│      412.58│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │    200│       100│         0│        99│            18│        22│        25│        28│        74│        77│        100.00│     25.57│      782.07│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │    300│       100│         1│       131│            16│        34│        38│        41│        64│        91│        100.00│     26.81│     1119.10│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │    500│       100│         0│       236│            17│        59│        66│        68│        73│       124│        100.00│     29.64│     1686.89│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │    750│       100│         1│       314│            16│        92│        99│       102│       106│       123│        100.00│     33.11│     2265.10│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │    800│       100│         1│       292│            18│        92│       107│       111│       116│       134│        100.00│     34.23│     2336.90│
    // └───────┴──────────┴──────────┴──────────┴──────────────┴──────────┴──────────┴──────────┴──────────┴──────────┴──────────────┴──────────┴────────────┘
    //
    // Handshake latencies
    // ┌───────┬──────────┬──────────┬──────────┬──────────────┬──────────┬──────────┬──────────┬──────────┬──────────┬──────────────┬──────────┬────────────┐
    // │ peers │ requests │ min (ms) │ max (ms) │ std dev (ms) │ 10% (ms) │ 50% (ms) │ 75% (ms) │ 90% (ms) │ 99% (ms) │ completion % │ time (s) │ requests/s │
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │      1│         1│         0│         0│             0│         0│         0│         0│         0│         0│        100.00│      2.03│        0.49│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │     10│         1│         1│         2│             1│         1│         1│         1│         2│         2│        100.00│     22.90│        0.44│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │     20│         1│         1│         7│             3│         1│         4│         5│         7│         7│        100.00│      3.22│        6.20│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │     30│         1│         0│        77│            14│         1│         3│         5│         8│        77│        100.00│     23.62│        1.27│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │     40│         1│         0│        18│             5│         1│         7│        11│        14│        18│        100.00│     23.47│        1.70│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │     50│         1│         0│      1896│           320│      1890│      1891│      1893│      1893│      1896│        100.00│      5.94│        8.41│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │     60│         1│         1│       122│            16│         2│        10│        16│        23│       122│        100.00│     23.50│        2.55│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │     70│         1│         0│        24│             8│         1│        11│        16│        23│        24│        100.00│     24.20│        2.89│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │     80│         1│         0│        29│            10│         2│        16│        22│        28│        29│        100.00│     24.09│        3.32│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │     90│         1│         0│        35│            11│         2│        20│        27│        34│        35│        100.00│     24.09│        3.74│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │    100│         1│         0│        43│            13│         2│        19│        33│        34│        43│        100.00│     24.24│        4.13│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │    200│         1│         0│       100│            25│         7│        73│        76│        79│       100│        100.00│     25.57│        7.82│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │    300│         1│         1│       171│            50│        11│        51│        81│       165│       170│        100.00│     26.81│       11.19│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │    500│         1│         1│       419│           111│        25│       128│       189│       361│       418│        100.00│     29.64│       16.87│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │    750│         1│         0│       589│           152│        59│       168│       297│       479│       588│        100.00│     33.11│       22.65│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────────┼──────────┼────────────┤
    // │    800│         1│         0│       772│           200│        52│       262│       433│       586│       680│        100.00│     34.23│       23.37│
    // └───────┴──────────┴──────────┴──────────┴──────────────┴──────────┴──────────┴──────────┴──────────┴──────────┴──────────────┴──────────┴────────────┘

    // enable simple metrics recording
    recorder::enable_simple_recorder().unwrap();

    const TIMEOUT: tokio::time::Duration = tokio::time::Duration::from_secs(20);

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
    let mut node = Node::new().unwrap();
    node.initial_action(Action::SeedWithTestnetBlocks(3))
        .max_peers(synth_counts.iter().max().unwrap() * 2 + 10)
        .start()
        .await
        .unwrap();

    let node_addr = node.addr();

    // Iterate over peer counts
    // (note: rng usage should stay in a single thread in order for it be somewhat repeatable - we can't account
    // for relative timings and state transitions in the node).
    for peers in synth_counts {
        let iteration_timer = tokio::time::Instant::now();
        // register metrics
        recorder::clear();
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
            let valid = (0..MAX_VALID_MESSAGES)
                .map(|_| valid_pool.choose(&mut rng).unwrap().clone())
                .collect::<Vec<_>>();
            // grab the broken message for this peer
            let corrupt = corrupt_messages.pop().unwrap();

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
        let request_latencies = recorder::histograms()
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
        let handshake_latencies = recorder::histograms()
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
            let counters = recorder::counters();
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
    println!("Stats\n{}\n", fmt_table(Table::new(stats)));
    println!("Request latencies\n{}\n", request_table);
    println!("Handshake latencies\n{}\n", handshake_table);

    node.stop().await.unwrap();
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

    // send the valid query messages and validate the responses
    for (query, expected) in message_pairs {
        if synth_node
            .send_direct_message(node_addr, query)
            .await
            .is_err()
        {
            metrics::counter!(CONNECTION_TERMINATED, 1);
            return;
        }

        let timer = tokio::time::Instant::now();
        let (_, reply) = synth_node.recv_message().await;
        metrics::histogram!(REQUEST_LATENCY, duration_as_ms(timer.elapsed()));
        assert_eq!(reply, expected);
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
