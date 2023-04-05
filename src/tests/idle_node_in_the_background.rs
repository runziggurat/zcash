//! Development test suite
//!
//! Some helper tools:
//!  - Change process niceness:
//!     sudo renice -n -19 -p $(pidof rippled)
//!
use std::net::SocketAddr;

use chrono::{DateTime, Utc};
use ziggurat_core_utils::err_constants::{ERR_NODE_BUILD, ERR_SYNTH_CONNECT};

use crate::{
    setup::node::{Action, Node},
    tools::synthetic_node::{self, SyntheticNode},
};

// Directory files are always placed under ~/.ziggurat since
// this implementation doesn't support arbitrary paths.
/// The directory where node data gets stored.
const NODE_DIR_PATH: &str = "~/.ziggurat";

#[derive(Default)]
enum NodeLogToStdout {
    #[default]
    Off,
    On,
}

impl NodeLogToStdout {
    fn is_on(&self) -> bool {
        matches!(self, NodeLogToStdout::On)
    }
}

#[derive(PartialEq, Default)]
enum TracingOpt {
    #[default]
    Off,
    On,
}

#[derive(Default)]
#[allow(non_camel_case_types)]
enum SyntheticNodeOpt {
    #[default]
    Off,
    On_OnlyListening,
    On_TryToConnect,
}

/// A simple configuration for the dev test customization.
#[derive(Default)]
struct DevTestCfg {
    /// Print the node's log to the stdout.
    log_to_stdout: NodeLogToStdout,

    /// Enable tracing.
    tracing: TracingOpt,

    // Attach a synthetic node to the node.
    synth_node: SyntheticNodeOpt,
}

#[tokio::test]
#[allow(non_snake_case)]
#[ignore = "convenience test to tinker with a running node for dev purposes"]
async fn dev001_t1_RUN_NODE_FOREVER_with_logs() {
    // This test is used for testing/development purposes.

    let cfg = DevTestCfg {
        log_to_stdout: NodeLogToStdout::On,
        tracing: TracingOpt::On,
        ..Default::default()
    };
    node_run_forever(cfg).await;

    panic!("the node shouldn't have died");
}

#[tokio::test]
#[allow(non_snake_case)]
#[ignore = "convenience test to tinker with a running node for dev purposes"]
async fn dev001_t2_RUN_NODE_FOREVER_no_logs() {
    // This test is used for testing/development purposes.

    let cfg = DevTestCfg::default();
    node_run_forever(cfg).await;

    panic!("the node shouldn't have died");
}

#[tokio::test]
#[allow(non_snake_case)]
#[ignore = "convenience test to tinker with a running node for dev purposes"]
async fn dev002_t1_MONITOR_NODE_FOREVER_WITH_SYNTH_NODE_sn_is_conn_initiator() {
    // This test is used for testing/development purposes.

    let cfg = DevTestCfg {
        synth_node: SyntheticNodeOpt::On_TryToConnect,
        ..Default::default()
    };
    node_run_forever(cfg).await;

    panic!("the node shouldn't have died");
}

#[tokio::test]
#[allow(non_snake_case)]
#[ignore = "convenience test to tinker with a running node for dev purposes"]
async fn dev002_t2_MONITOR_NODE_FOREVER_WITH_SYNTH_NODE_sn_is_conn_responder() {
    // This test is used for testing/development purposes.

    let cfg = DevTestCfg {
        log_to_stdout: NodeLogToStdout::On,
        tracing: TracingOpt::On,
        synth_node: SyntheticNodeOpt::On_OnlyListening,
    };
    node_run_forever(cfg).await;

    panic!("the node shouldn't have died");
}

/// Runs the node forever!
/// The test asserts the node process won't be killed.
///
/// Function complexity is increased due to many customization options,
/// which is not nice but it is what it is.
///
/// In short, here are the customization options which are provided via the cfg arg:
///
///  - enable/disable node's logs to stdout [cfg.log_to_stdout]
///
///  - enable/disable tracing [cfg.tracing]
///
///  - enable/disable attaching a single synthetic node to the node [cfg.synth_node]
///    - suboption: choose the initiator for the connection
///
async fn node_run_forever(cfg: DevTestCfg) {
    let log_to_stdout = cfg.log_to_stdout.is_on();

    // Enable tracing possibly.
    if cfg.tracing == TracingOpt::On {
        synthetic_node::enable_tracing();
    }

    // SyntheticNode is spawned only if option is chosen in cfg options.
    let mut initial_peers = vec![];
    let mut synth_node: Option<SyntheticNode> = match cfg.synth_node {
        SyntheticNodeOpt::On_TryToConnect => {
            let synthetic_node = SyntheticNode::builder()
                .with_full_handshake()
                .build()
                .await
                .unwrap();
            Some(synthetic_node)
        }
        SyntheticNodeOpt::On_OnlyListening => {
            let synthetic_node = SyntheticNode::builder()
                .with_full_handshake()
                .build()
                .await
                .unwrap();
            initial_peers.push(synthetic_node.listening_addr());
            Some(synthetic_node)
        }
        _ => None,
    };

    let mut node = node_start(log_to_stdout, initial_peers.clone()).await;
    let addr = node.addr();

    if let Some(synth_node) = synth_node.as_ref() {
        // Alternative check to the On_TryToConnect option.
        if initial_peers.is_empty() {
            synth_node.connect(addr).await.expect(ERR_SYNTH_CONNECT);
        }
    }

    // Print received messages from another thread.
    if let Some(synth_node) = synth_node.take() {
        spawn_periodic_msg_recv(synth_node).await;
    }

    // The node should run forever unless something bad happens to it.
    node.wait_until_exit().await;

    println!("\tThe node has stopped running ({})", current_time_str());
}

/// Create and start the node and print the extra useful debug info.
async fn node_start(log_to_stdout: bool, initial_peers: Vec<SocketAddr>) -> Node {
    println!("\tTime before the node is started: {}", current_time_str());

    let mut node = Node::new().unwrap();
    node.initial_action(Action::WaitForConnection)
        .log_to_stdout(log_to_stdout)
        .initial_peers(initial_peers.clone())
        .start()
        .await
        .expect(ERR_NODE_BUILD);

    println!("\tThe node directory files are located at {NODE_DIR_PATH}");
    println!("\tThe node has started running ({})", current_time_str());
    println!("\tInitial peers: {initial_peers:?}");
    println!("\tThe node is listening on {}", node.addr());

    if !log_to_stdout {
        let log_path = format!("{NODE_DIR_PATH}/testnet3/debug.log");
        println!("\tThe node logs can be found at {log_path}");
    }

    node
}

/// Use recv_message to clear up the inbound queue and print out
/// the received messages.
async fn spawn_periodic_msg_recv(mut synth_node: SyntheticNode) {
    tokio::spawn(async move {
        loop {
            let (_, msg) = synth_node.recv_message().await;
            tracing::info!("message received: {msg:?}");
        }
    });
}

fn current_time_str() -> String {
    let now: DateTime<Utc> = Utc::now();
    now.format("%T %a %b %e %Y").to_string()
}
