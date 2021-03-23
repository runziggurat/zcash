// Zebra and Zcashd node setup:
//
// - setup::node::start
// - setup::node::teardown
//
//  Start will need:
//
//  - path to compiled node
//  - start command
//  - node configuration (this could be tricky as it usually done through a file)
//  - node cache instantiation
//  - docker considerations?
//
//
//  API:
//
//  - Listening port address
//  - Initial peers (testnet)
//  - Peerset initial target size
//  - New peer interval
//
//  Stop will need:
//
//  - stop command (kill it from rust)
//  - delete caches

use serde::{Deserialize, Serialize};
use tokio::process::{Child, Command};

use std::{
    collections::HashSet,
    env,
    ffi::OsString,
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
    process::Stdio,
};

pub struct NodeConfig {
    listening_address: SocketAddr,
    initial_peers: HashSet<SocketAddr>,
    max_peers: usize,
    log_to_stdout: bool,
}

impl NodeConfig {
    pub fn new(listening_address: SocketAddr, initial_peers: HashSet<SocketAddr>) -> Self {
        Self {
            listening_address,
            initial_peers,
            max_peers: 50,
            log_to_stdout: true,
        }
    }

    fn generate_file(&self, path: &Path, node_type: NodeType) {
        let path = path.join("node.toml");
        let content = match node_type {
            NodeType::Zebra => ZebraConfig::generate_from(self),
            NodeType::Zcashd => unimplemented!(),
        };

        fs::write(path, content).unwrap();
    }
}

#[derive(Deserialize, Debug)]
enum NodeType {
    Zebra,
    Zcashd,
}

#[derive(Deserialize, Debug)]
struct ZigguratConfig {
    node_type: NodeType,
    path: PathBuf,
    command: String,
}

pub async fn start(node_config: NodeConfig) -> Child {
    // 1. Read Ziggurat config for start, stop etc...
    // 2. Generate appropriate node config files (zebra, zcashd).
    // 3. Start node with generated config file.

    // Read the config file.
    let config_path = env::current_dir().unwrap().join("config.toml");
    let content = fs::read_to_string(config_path).unwrap();
    let config_file: ZigguratConfig = toml::from_str(&content).unwrap();

    // Contains the command and args.
    let mut args: Vec<OsString> = config_file
        .command
        .split_whitespace()
        .map(|s| OsString::from(s))
        .collect();

    // Extract the command from the vec.
    let command = args.remove(0);
    let path = PathBuf::from(&config_file.path);

    // Generate config files for Zebra or Zcashd node.
    node_config.generate_file(&config_file.path, config_file.node_type);

    let stdout = match node_config.log_to_stdout {
        true => Stdio::inherit(),
        false => Stdio::null(),
    };

    let process = Command::new(command)
        .current_dir(path)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(stdout)
        .kill_on_drop(true)
        .spawn()
        .expect("node failed to start");

    // In future maybe ping to check if ready? Maybe in include an explicit build step here as
    // well?
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;

    process
}

pub async fn stop(mut process: Child) {
    // TODO: remove node config file?
    process.kill().await.expect("failed to kill process");
}

// ZEBRA

#[derive(Serialize)]
struct NetworkConfig {
    listen_addr: SocketAddr,
    initial_testnet_peers: HashSet<String>,
    peerset_initial_target_size: usize,
    network: String,
}

impl NetworkConfig {
    fn new(listening_address: SocketAddr, initial_peers: &HashSet<SocketAddr>) -> Self {
        let initial_testnet_peers: HashSet<String> =
            initial_peers.iter().map(|addr| addr.to_string()).collect();

        Self {
            listen_addr: listening_address,
            initial_testnet_peers,
            peerset_initial_target_size: 50,
            network: String::from("Testnet"),
        }
    }
}

#[derive(Serialize)]
struct StateConfig {
    cache_dir: Option<String>,
    ephemeral: bool,
}

#[derive(Serialize)]
struct TracingConfig {
    filter: Option<String>,
}

#[derive(Serialize)]
pub struct ZebraConfig {
    network: NetworkConfig,
    state: StateConfig,
    tracing: TracingConfig,
}

impl ZebraConfig {
    fn generate_from(config: &NodeConfig) -> String {
        let zebra_config = Self {
            network: NetworkConfig::new(config.listening_address, &config.initial_peers),
            state: StateConfig {
                cache_dir: None,
                ephemeral: true,
            },
            tracing: TracingConfig {
                filter: Some("zebra_network=trace".to_string()),
            },
        };

        toml::to_string(&zebra_config).unwrap()
    }
}

// ZCASHD
