use serde::{Deserialize, Serialize};

use std::{
    collections::HashSet,
    ffi::OsString,
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
};

pub struct NodeConfig {
    pub listening_address: SocketAddr,
    pub initial_peers: HashSet<SocketAddr>,
    pub max_peers: usize,
    pub log_to_stdout: bool,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            listening_address: "127.0.0.1:8080".parse().unwrap(),
            initial_peers: HashSet::new(),
            max_peers: 50,
            log_to_stdout: false,
        }
    }
}

#[derive(Deserialize, Debug)]
pub enum NodeKind {
    Zebra,
    Zcashd,
}

#[derive(Deserialize, Debug)]
struct NodeMetaFile {
    kind: NodeKind,
    path: PathBuf,
    command: String,
}

pub struct NodeMeta {
    pub kind: NodeKind,
    pub path: PathBuf,
    pub command: OsString,
    pub args: Vec<OsString>,
}

impl NodeMeta {
    pub(super) fn new(path: &Path) -> Self {
        let meta_string = fs::read_to_string(path).unwrap();
        let meta_file: NodeMetaFile = toml::from_str(&meta_string).unwrap();

        let mut args: Vec<OsString> = meta_file
            .command
            .split_whitespace()
            .map(|s| OsString::from(s))
            .collect();

        // Extract the command from the vec.
        let command = args.remove(0);

        Self {
            kind: meta_file.kind,
            path: meta_file.path,
            command,
            args,
        }
    }
}

// ZEBRA CONFIG FILE

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
pub(super) struct ZebraConfig {
    network: NetworkConfig,
    state: StateConfig,
    tracing: TracingConfig,
}

impl ZebraConfig {
    pub(super) fn generate(config: &NodeConfig) -> String {
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
