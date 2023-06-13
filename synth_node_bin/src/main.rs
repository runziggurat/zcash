//! A synthetic node binary can be used to interact with the node in the
//! background from a different runtime environment.
use std::{net::SocketAddr, process::ExitCode};

use action::{ActionHandler, ActionType};
use anyhow::Result;
use clap::Parser;
use ziggurat_zcash::tools::synthetic_node::SyntheticNode;

use crate::ActionType::SendGetAddrAndForeverSleep;

mod action;

/// A synthetic node which can connect to the node and preform some actions independently.
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct CmdArgs {
    /// An address of the node in the <ip>:<port> format.
    #[arg(short = 'n', long)]
    node_addr: Option<SocketAddr>,

    /// Always reconnect in the case the connection fails - synthetic node never dies.
    #[arg(short = 's', long, default_value_t = false)]
    stubborn: bool,

    /// Enable tracing.
    #[arg(short = 't', long, default_value_t = false)]
    tracing: bool,

    /// A desired listening port.
    #[arg(short = 'p', long)]
    desired_listening_port: Option<u16>,

    /// Possible actions:
    /// SendGetAddrAndForeverSleep / AdvancedSnForS001 / QuickConnectAndThenCleanDisconnect /
    /// QuickConnectWithImproperDisconnect / ConstantlyAskForRandomBlocks / RtS1Collector / RtS1Tainter
    #[arg(short = 'a', long, default_value_t = SendGetAddrAndForeverSleep)]
    action_type: ActionType,
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = CmdArgs::parse();
    let node_addr = args.node_addr;

    if args.tracing {
        println!("Enabling tracing.");
        use tracing_subscriber::{fmt, EnvFilter};

        fmt()
            .with_test_writer()
            .with_env_filter(EnvFilter::from_default_env())
            .init();
    }

    loop {
        println!("Starting a synthetic node.");

        if let Err(e) =
            run_synth_node(node_addr, args.action_type, args.desired_listening_port).await
        {
            eprintln!("The synthetic node stopped: {e:?}.");
        }

        // Use the stubborn option to run the synth node infinitely.
        if !args.stubborn {
            break;
        }
    }

    ExitCode::SUCCESS
}

async fn run_synth_node(
    node_addr: Option<SocketAddr>,
    action_type: ActionType,
    desired_listening_port: Option<u16>,
) -> Result<()> {
    // Select an action.
    let action = ActionHandler::new(action_type);

    let mut net_cfg = action.cfg.network_cfg.clone();
    // A user can always override a default value from an action.
    if desired_listening_port.is_some() {
        net_cfg.desired_listening_port = desired_listening_port;
    }

    // Create a synthetic node and enable handshaking.
    let mut synth_node = SyntheticNode::builder()
        .with_network_config(net_cfg)
        .with_full_handshake()
        .with_message_filter(action.cfg.msg_filter.clone())
        .build()
        .await?;

    // Perform the handshake.
    if let Some(addr) = node_addr {
        synth_node.connect(addr).await?;
    }

    // Run the wanted action with the node.
    action.execute(&mut synth_node, node_addr).await?;

    if action.cfg.allow_proper_shutdown {
        // Stop the synthetic node.
        synth_node.shut_down().await;
    }

    Ok(())
}
