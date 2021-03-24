use serde::{Deserialize, Serialize};

use std::{
    collections::HashSet,
    ffi::OsString,
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
};

/// Node configuration used for creating a `Node` instance.
///
/// The information contained in this struct will be written to a config file read by the node at
/// start time and as such can only contain options shared by all types of node.
pub struct NodeConfig {
    /// The node's listening address.
    pub local_addr: SocketAddr,
    /// The initial peerset to connect to on node start.
    pub initial_peers: HashSet<SocketAddr>,
    /// The initial max number of peer connections to allow.
    pub max_peers: usize,
    /// Setting this option to true will enable node logging to stdout.
    pub log_to_stdout: bool,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            local_addr: "127.0.0.1:8080".parse().unwrap(),
            initial_peers: HashSet::new(),
            max_peers: 50,
            log_to_stdout: false,
        }
    }
}

/// Describes the node kind, currently supports the two known variants.
#[derive(Deserialize, Debug)]
#[serde(rename_all(deserialize = "lowercase"))]
pub enum NodeKind {
    Zebra,
    Zcashd,
}

/// Node metadata read from the `config.toml` configuration file.
pub struct NodeMetaData {
    /// The node kind (one of `Zebra` or `Zcashd`).
    pub kind: NodeKind,
    /// The path to run the node's commands in.
    pub path: PathBuf,
    /// The command to run when starting a node.
    pub start_command: OsString,
    /// The args to run with the start command.
    pub start_args: Vec<OsString>,
    /// The command to run when stopping a node.
    pub stop_command: Option<OsString>,
    /// The args to run with the stop command.
    pub stop_args: Option<Vec<OsString>>,
}

impl NodeMetaData {
    pub(super) fn new(path: &Path) -> Self {
        let meta_string = fs::read_to_string(path).unwrap();
        let meta_file: MetaDataFile = toml::from_str(&meta_string).unwrap();

        let args_from = |command: &str| -> Vec<OsString> {
            command.split_whitespace().map(OsString::from).collect()
        };

        let mut start_args = args_from(&meta_file.start_command);
        let start_command = start_args.remove(0);

        // Default to None.
        let mut stop_args = None;
        let mut stop_command = None;

        if let Some(command) = meta_file.stop_command {
            let mut args = args_from(&command);
            stop_command = Some(args.remove(0));
            stop_args = Some(args);
        }

        Self {
            kind: meta_file.kind,
            path: meta_file.path,
            start_command,
            start_args,
            stop_command,
            stop_args,
        }
    }
}

/// Convenience struct for reading the toml configuration file. The data read here is used to
/// construct a `NodeMeta` instance.
#[derive(Deserialize, Debug)]
struct MetaDataFile {
    kind: NodeKind,
    path: PathBuf,
    start_command: String,
    stop_command: Option<String>,
}

// ZEBRA CONFIG FILE

/// Convenience struct for writing a zebra compatible configuration file.
#[derive(Serialize)]
pub(super) struct ZebraConfigFile {
    network: NetworkConfig,
    state: StateConfig,
    tracing: TracingConfig,
}

impl ZebraConfigFile {
    /// Generate the toml configuration as a string.
    pub(super) fn generate(config: &NodeConfig) -> String {
        // Create the structs to prepare for encoding.
        let initial_testnet_peers: HashSet<String> = config
            .initial_peers
            .iter()
            .map(|addr| addr.to_string())
            .collect();

        let zebra_config = Self {
            network: NetworkConfig {
                listen_addr: config.local_addr,
                initial_testnet_peers,
                peerset_initial_target_size: config.max_peers,
                network: String::from("Testnet"),
            },
            state: StateConfig {
                cache_dir: None,
                ephemeral: true,
            },
            tracing: TracingConfig {
                filter: Some("zebra_network=trace".to_string()),
            },
        };

        // Write the toml to a string.
        toml::to_string(&zebra_config).unwrap()
    }
}

#[derive(Serialize)]
struct NetworkConfig {
    listen_addr: SocketAddr,
    initial_testnet_peers: HashSet<String>,
    peerset_initial_target_size: usize,
    network: String,
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

/// Convenience struct for writing a zcashd compatible configuration file.
pub(super) struct ZcashdConfigFile;

impl ZcashdConfigFile {
    pub(super) fn generate(config: &NodeConfig) -> String {
        let mut contents = format!(
            "testnet=1\nbind={}\nmaxconnections={}\n",
            config.local_addr, config.max_peers
        );

        if config.initial_peers.is_empty() {
            contents.push_str("connect=\n")
        } else {
            for peer in &config.initial_peers {
                contents.push_str(&format!("connect={}\n", peer))
            }
        }

        contents
    }
}
