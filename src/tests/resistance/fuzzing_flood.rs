use rand::{prelude::SliceRandom, Rng};
use rand_chacha::ChaCha8Rng;
use std::{io::Write, net::SocketAddr};
use tabled::{table, Alignment, Style, Tabled};

use crate::{
    helpers::{initiate_handshake, is_rejection_error, is_termination_error},
    protocol::{
        message::{constants::MAGIC, filter::MessageFilter, Message, MessageHeader},
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
    #[header(" handshake \n errors ")]
    handshake_errors: u16,
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
    #[header(" io errors ")]
    io_errors: u16,

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
const HANDSHAKE_ERROR: &str = "fuzz_flood_handshake_error";

const CONNECTION_TERMINATED: &str = "fuzz_flood_connection_terminated";
const IO_ERROR: &str = "fuzz_flood_io_error";
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
    // ┌─────┬────────┬────────────┬────────────┬───────────┬─────────┬────────────┬──────────┬─────────┬─────────┬─────────┬───────────┬─────────────┬──────────┐
    // │     │        │ handshakes │ handshakes │ handshake │  peers  │   corrupt  │  corrupt │ corrupt │ corrupt │   bad   │           │     hung    │          │
    // │peers│requests│  accepted  │  rejected  │   errors  │ dropped │ terminated │ rejected │ replied │ ignored │ replies │ io errors │ connections │ time (s) │
    // ├─────┼────────┼────────────┼────────────┼───────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼───────────┼─────────────┼──────────┤
    // │    1│     100│           1│           0│          0│        0│           0│         0│        0│        1│        0│          0│            0│      0.02│
    // ├─────┼────────┼────────────┼────────────┼───────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼───────────┼─────────────┼──────────┤
    // │   10│     100│          10│           0│          0│        0│           8│         0│        0│        1│        0│          0│            1│     11.27│
    // ├─────┼────────┼────────────┼────────────┼───────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼───────────┼─────────────┼──────────┤
    // │   20│     100│          20│           0│          0│        0│          13│         1│        0│        5│        0│          0│            1│     23.04│
    // ├─────┼────────┼────────────┼────────────┼───────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼───────────┼─────────────┼──────────┤
    // │   30│     100│          30│           0│          0│        0│          24│         1│        0│        5│        0│          0│            0│     24.17│
    // ├─────┼────────┼────────────┼────────────┼───────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼───────────┼─────────────┼──────────┤
    // │   40│     100│          40│           0│          0│        0│          30│         1│        0│        6│        0│          0│            3│     34.98│
    // ├─────┼────────┼────────────┼────────────┼───────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼───────────┼─────────────┼──────────┤
    // │   50│     100│          50│           0│          0│        0│          38│         2│        0│        7│        0│          0│            3│     45.82│
    // ├─────┼────────┼────────────┼────────────┼───────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼───────────┼─────────────┼──────────┤
    // │   60│     100│          60│           0│          0│        0│          45│         2│        0│        8│        0│          0│            5│     56.85│
    // ├─────┼────────┼────────────┼────────────┼───────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼───────────┼─────────────┼──────────┤
    // │   70│     100│          70│           0│          0│        0│          52│         5│        0│        7│        0│          0│            6│     68.14│
    // ├─────┼────────┼────────────┼────────────┼───────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼───────────┼─────────────┼──────────┤
    // │   80│     100│          80│           0│          0│        0│          64│         2│        0│       12│        0│          0│            2│     79.46│
    // ├─────┼────────┼────────────┼────────────┼───────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼───────────┼─────────────┼──────────┤
    // │   90│     100│          90│           0│          0│        0│          72│         3│        0│       12│        0│          0│            3│     90.72│
    // ├─────┼────────┼────────────┼────────────┼───────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼───────────┼─────────────┼──────────┤
    // │  100│     100│         100│           0│          0│        0│          71│         5│        0│       18│        0│          0│            6│    101.87│
    // ├─────┼────────┼────────────┼────────────┼───────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼───────────┼─────────────┼──────────┤
    // │  200│     100│         200│           0│          0│        0│         153│        16│        0│       21│        0│          0│           10│    113.93│
    // ├─────┼────────┼────────────┼────────────┼───────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼───────────┼─────────────┼──────────┤
    // │  300│     100│         300│           0│          0│        0│         239│        15│        0│       33│        0│          0│           13│    126.93│
    // ├─────┼────────┼────────────┼────────────┼───────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼───────────┼─────────────┼──────────┤
    // │  500│     100│         500│           0│          0│        0│         395│        23│        0│       55│        0│          0│           27│    141.56│
    // ├─────┼────────┼────────────┼────────────┼───────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼───────────┼─────────────┼──────────┤
    // │  750│     100│         750│           0│          0│        0│         552│        53│        0│       94│        0│          0│           51│    158.00│
    // ├─────┼────────┼────────────┼────────────┼───────────┼─────────┼────────────┼──────────┼─────────┼─────────┼─────────┼───────────┼─────────────┼──────────┤
    // │  800│     100│         800│           0│          0│        0│         593│        56│        0│      105│        0│          0│           46│    174.80│
    // └─────┴────────┴────────────┴────────────┴───────────┴─────────┴────────────┴──────────┴─────────┴─────────┴─────────┴───────────┴─────────────┴──────────┘
    //
    // Request latencies
    // ┌───────┬──────────┬──────────┬──────────┬──────────────┬──────────┬──────────┬──────────┬──────────┬──────────┬──────────┬────────────┐
    // │ peers │ requests │ min (ms) │ max (ms) │ std dev (ms) │ 10% (ms) │ 50% (ms) │ 75% (ms) │ 90% (ms) │ 99% (ms) │ time (s) │ requests/s │
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │      1│       100│         0│         0│             0│         0│         0│         0│         0│         0│      0.02│     4362.10│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     10│       100│         0│       101│            24│         0│         1│        49│        51│        52│     11.27│       88.74│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     20│       100│         0│       764│            34│         0│         1│         2│        51│       102│     23.04│       86.81│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     30│       100│         0│       102│            18│         1│         3│         4│        51│        55│     24.17│      124.14│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     40│       100│         0│        60│            12│         1│         3│         5│         8│        55│     34.98│      114.36│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     50│       100│         0│       107│            19│         1│         5│         6│        11│       106│     45.82│      109.12│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     60│       100│         0│        59│            14│         2│         5│         7│        29│        57│     56.85│      105.53│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     70│       100│         0│        85│            18│         2│         6│         9│        52│        61│     68.14│      102.73│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     80│       100│         0│        96│            18│         2│         7│         9│        52│        60│     79.46│      100.68│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     90│       100│         0│       101│            13│         3│         8│        10│        22│        53│     90.72│       99.20│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │    100│       100│         0│        66│            15│         2│         8│        11│        28│        63│    101.87│       98.17│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │    200│       100│         0│       102│            14│         7│        17│        22│        45│        63│    113.93│      175.55│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │    300│       100│         0│       125│            19│        11│        29│        37│        57│        91│    126.93│      236.35│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │    500│       100│         0│       164│            23│        20│        49│        64│        73│       112│    141.56│      353.20│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │    750│       100│         0│       203│            27│        30│        67│        82│        89│       154│    158.00│      474.67│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │    800│       100│         0│       187│            26│        29│        64│        77│        88│       153│    174.80│      457.66│
    // └───────┴──────────┴──────────┴──────────┴──────────────┴──────────┴──────────┴──────────┴──────────┴──────────┴──────────┴────────────┘
    //
    // Handshake latencies
    // ┌───────┬──────────┬──────────┬──────────┬──────────────┬──────────┬──────────┬──────────┬──────────┬──────────┬──────────┬────────────┐
    // │ peers │ requests │ min (ms) │ max (ms) │ std dev (ms) │ 10% (ms) │ 50% (ms) │ 75% (ms) │ 90% (ms) │ 99% (ms) │ time (s) │ requests/s │
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │      1│         1│         0│         0│             0│         0│         0│         0│         0│         0│      0.02│       43.62│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     10│         1│         0│         4│             2│         0│         1│         3│         4│         4│     11.27│        0.89│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     20│         1│         0│       764│           167│       762│       763│       764│       764│       764│     23.04│        0.87│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     30│         1│         0│        17│             6│         1│         5│        13│        14│        17│     24.17│        1.24│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     40│         1│         0│        13│             5│         1│         6│        10│        12│        13│     34.98│        1.14│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     50│         1│         0│        90│            40│         1│        11│        86│        89│        90│     45.82│        1.09│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     60│         1│         0│       101│            27│         1│        11│        18│        88│       101│     56.85│        1.06│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     70│         1│         0│        30│             9│         2│        12│        23│        24│        30│     68.14│        1.03│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     80│         1│         0│        46│            14│         1│        16│        31│        37│        46│     79.46│        1.01│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │     90│         1│         0│        49│            14│         3│        18│        31│        40│        49│     90.72│        0.99│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │    100│         1│         0│        32│            11│         3│        18│        25│        31│        32│    101.87│        0.98│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │    200│         1│         0│       136│            41│        33│        36│       111│       133│       135│    113.93│        1.76│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │    300│         1│         0│       287│            57│        43│        46│        49│       147│       286│    126.93│        2.36│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │    500│         1│         0│       668│           168│        24│       189│       319│       462│       667│    141.56│        3.53│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │    750│         1│         0│       970│           299│        56│       339│       660│       813│       887│    158.00│        4.75│
    // ├───────┼──────────┼──────────┼──────────┼──────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼────────────┤
    // │    800│         1│         2│      1775│           562│        57│       629│      1291│      1436│      1774│    174.80│        4.58│
    // └───────┴──────────┴──────────┴──────────┴──────────────┴──────────┴──────────┴──────────┴──────────┴──────────┴──────────┴────────────┘

    // enable simple metrics recording
    simple_metrics::enable_simple_recorder().unwrap();

    const TIMEOUT: tokio::time::Duration = tokio::time::Duration::from_secs(10);

    let mut request_table = RequestsTable::default();
    let mut handshake_table = RequestsTable::default();
    let mut stats = Vec::<Stats>::new();

    // Create a pool of valid and invalid message types
    const MAX_VALID_MESSAGES: usize = 100;
    let peer_counts = vec![
        1, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 200, 300, 500, 750, 800,
    ];

    let mut rng = seeded_rng();
    let valid_pool = valid_queries_responses();

    // Start node with arbitrarily higher max peer count than what we
    // need for the test. Note that zcashd node appears to reserver 8
    // slots (hence the +10).
    let mut node: Node = Default::default();
    node.initial_action(Action::SeedWithTestnetBlocks(3))
        .max_peers(peer_counts.iter().max().unwrap() * 2 + 10)
        .start()
        .await;

    let node_addr = node.addr();

    let iteration_timer = tokio::time::Instant::now();

    // Iterate over peer counts
    // (note: rng usage should stay in a single thread in order for it be somewhat repeatable - we can't account
    // for relative timings and state transitions in the node).
    for peers in peer_counts {
        // register metrics
        simple_metrics::clear();
        metrics::register_histogram!(REQUEST_LATENCY);
        metrics::register_histogram!(HANDSHAKE_LATENCY);

        metrics::register_counter!(HANDSHAKE_ACCEPTED);
        metrics::register_counter!(HANDSHAKE_REJECTED);
        metrics::register_counter!(HANDSHAKE_ERROR);
        metrics::register_counter!(CONNECTION_TERMINATED);
        metrics::register_counter!(IO_ERROR);
        metrics::register_counter!(BAD_REPLY);
        metrics::register_counter!(CORRUPT_TERMINATED);
        metrics::register_counter!(CORRUPT_REJECTED);
        metrics::register_counter!(CORRUPT_REPLY);
        metrics::register_counter!(CORRUPT_IGNORED);

        // Generate the broken fuzz messages for this peer set (one per peer, since this should break the connection)
        let mut corrupt_messages = generate_corrupt_messages(&mut rng, peers);

        let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(peers);

        // Start the N peers for this iteration
        let mut peer_handles = Vec::with_capacity(peers);
        let mut peer_exits = Vec::with_capacity(peers);
        for _ in 0..peers {
            let (exit_tx, exit_rx) = tokio::sync::oneshot::channel::<()>();
            peer_exits.push(exit_tx);

            let peer_complete_notifier = tx.clone();

            // generate the valid message set for this peer
            let valid = (0..rng.gen_range(0..MAX_VALID_MESSAGES))
                .map(|_| valid_pool.choose(&mut rng).unwrap().clone())
                .collect::<Vec<_>>();
            // grab the broken message for this peer
            let corrupt = corrupt_messages.pop().unwrap();

            // let peer_tx = event_tx.clone();

            // start and wait for peer simulation to complete, or cancel if exit signal is received
            peer_handles.push(tokio::spawn(async move {
                tokio::select! {
                    _ = exit_rx => {},
                    _ = simulate_peer(node_addr, valid, corrupt) => {},
                }
                peer_complete_notifier.send(()).await.unwrap();
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
        for exit in peer_exits {
            let _ = exit.send(());
        }

        // Wait for peers to complete
        for handle in peer_handles {
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
            stat.handshake_errors = locked_counters
                .get(&metrics::Key::from_name(HANDSHAKE_ERROR))
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

            stat.io_errors = locked_counters
                .get(&metrics::Key::from_name(IO_ERROR))
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

    let random_payloads = metadata_compliant_random_bytes(rng, n, &COMMANDS_WITH_PAYLOADS);
    for (header, payload) in random_payloads {
        let mut buffer = Vec::new();

        header.encode(&mut buffer).unwrap();
        buffer.write_all(&payload).unwrap();

        possible_payloads.push(buffer);
    }

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
    // handshake
    let timer = tokio::time::Instant::now();
    let mut stream = match initiate_handshake(node_addr).await {
        Ok(stream) => {
            metrics::counter!(HANDSHAKE_ACCEPTED, 1);
            metrics::histogram!(HANDSHAKE_LATENCY, duration_as_ms(timer.elapsed()));
            stream
        }
        Err(err) if is_rejection_error(&err) => {
            metrics::counter!(HANDSHAKE_REJECTED, 1);
            metrics::histogram!(HANDSHAKE_LATENCY, duration_as_ms(timer.elapsed()));
            return;
        }
        Err(err) => {
            metrics::counter!(HANDSHAKE_ERROR, 1);
            panic!("Unexpected error during handshake: {}", err);
        }
    };

    // send the valid messages and validate the response
    let filter = MessageFilter::with_all_auto_reply();
    for (msg, expected) in message_pairs {
        match msg.write_to_stream(&mut stream).await {
            Ok(_) => {}
            Err(err) if is_termination_error(&err) => {
                metrics::counter!(CONNECTION_TERMINATED, 1);
                return;
            }
            Err(err) => {
                metrics::counter!(IO_ERROR, 1);
                panic!("Error writing request message: {}", err);
            }
        }
        let timer = tokio::time::Instant::now();
        match filter.read_from_stream(&mut stream).await {
            Ok(reply) if reply == expected => {
                metrics::histogram!(REQUEST_LATENCY, duration_as_ms(timer.elapsed()));
            }
            Ok(reply) => {
                metrics::counter!(BAD_REPLY, 1);
                panic!("Bad reply received: {:?}", reply);
            }
            Err(err) if is_termination_error(&err) => {
                metrics::counter!(CONNECTION_TERMINATED, 1);
                return;
            }
            Err(err) => {
                metrics::counter!(IO_ERROR, 1);
                panic!("Error reading request response: {}", err);
            }
        }
    }

    // send the corrupt message and expect the connection to be terminated
    match tokio::io::AsyncWriteExt::write_all(&mut stream, &corrupt_message).await {
        Ok(_) => {}
        Err(err) if is_termination_error(&err) => {
            metrics::counter!(CORRUPT_TERMINATED, 1);
            return;
        }
        Err(err) => {
            metrics::counter!(IO_ERROR, 1);
            panic!("Error writing corrupt message: {}", err);
        }
    }
    //  check for termination by sending a ping -> pong (should result in a terminated connection prior to the pong)
    let nonce = Nonce::default();
    match Message::Ping(nonce).write_to_stream(&mut stream).await {
        Ok(_) => {}
        Err(err) if is_termination_error(&err) => {
            metrics::counter!(CORRUPT_TERMINATED, 1);
            return;
        }
        Err(err) => {
            metrics::counter!(IO_ERROR, 1);
            panic!("Error reading corrupt response: {}", err);
        }
    }
    match filter.read_from_stream(&mut stream).await {
        Ok(Message::Pong(rx_nonce)) if nonce == rx_nonce => {
            metrics::counter!(CORRUPT_IGNORED, 1);
        }
        Ok(Message::Reject(..)) => {
            metrics::counter!(CORRUPT_REJECTED, 1);
        }
        Ok(message) => {
            metrics::counter!(CORRUPT_REPLY, 1);
            panic!("Reply received to corrupt message: {:?}", message);
        }
        Err(err) if is_termination_error(&err) => {
            metrics::counter!(CORRUPT_TERMINATED, 1);
        }
        Err(err) => {
            metrics::counter!(IO_ERROR, 1);
            panic!("Error reading response to corrupt message: {}", err);
        }
    }
}
