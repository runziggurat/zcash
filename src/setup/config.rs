use serde::{Deserialize, Serialize};

use std::{
    collections::HashSet,
    env,
    ffi::OsString,
    fs,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
};

use crate::setup::node::Action;

const NODE_PORT: u16 = 8080;

/// Reads the contents of Ziggurat's configuration file.
pub fn read_config_file() -> NodeMetaData {
    let path = &env::current_dir().unwrap().join("config.toml");
    let config_string = fs::read_to_string(path).unwrap();
    let config_file: ConfigFile = toml::from_str(&config_string).unwrap();

    let node_meta = NodeMetaData::new(config_file.node);

    node_meta
}

/// Returns a new address suitable for starting a local listener.
pub fn new_local_addr() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)
}

/// Convenience struct for reading Ziggurat's configuration file.
#[derive(Deserialize)]
struct ConfigFile {
    local_ip: Option<String>,
    node: MetaDataFile,
}

/// Node configuration abstracted by a [`Node`] instance.
///
/// The information contained in this struct will be written to a config file read by the node at
/// start time and as such can only contain options shared by all types of node.
///
/// [`Node`]: struct@crate::setup::node::Node
pub(super) struct NodeConfig {
    /// The socket address of the node.
    pub(super) local_addr: SocketAddr,
    /// The initial peerset to connect to on node start.
    pub(super) initial_peers: HashSet<String>,
    /// The initial max number of peer connections to allow.
    pub(super) max_peers: usize,
    /// Setting this option to true will enable node logging to stdout.
    pub(super) log_to_stdout: bool,
    /// Defines the intial action to take once the node has started.
    pub(super) initial_action: Action,
}

impl NodeConfig {
    pub(super) fn new() -> Self {
        Self {
            local_addr: new_local_addr(),
            initial_peers: HashSet::new(),
            max_peers: 50,
            log_to_stdout: false,
            initial_action: Action::None,
        }
    }
}

/// Describes the node kind, currently supports the two known variants.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all(deserialize = "lowercase"))]
pub(super) enum NodeKind {
    Zebra,
    Zcashd,
}

/// Node configuration read from the `config.toml` file.
#[derive(Clone)]
pub struct NodeMetaData {
    /// The node kind (one of `Zebra` or `Zcashd`).
    pub(super) kind: NodeKind,
    /// The path to run the node's commands in.
    pub(super) path: PathBuf,

    /// The command to run when starting a node.
    pub(super) start_command: OsString,
    /// The args to run with the start command.
    pub(super) start_args: Vec<OsString>,
}

impl NodeMetaData {
    fn new(meta_file: MetaDataFile) -> Self {
        let args_from = |command: &str| -> Vec<OsString> {
            command.split_whitespace().map(OsString::from).collect()
        };

        let mut start_args = args_from(&meta_file.start_command);
        let start_command = start_args.remove(0);

        Self {
            kind: meta_file.kind,
            path: meta_file.path,
            start_command,
            start_args,
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
                // Set ip from config, port from assigned in `Config`.
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
                filter: Some("zebra_network=trace,zebrad=trace".to_string()),
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
            "testnet=1\nwhitebind={}\nmaxconnections={}\n",
            config.local_addr, config.max_peers
        );

        if config.initial_peers.is_empty() {
            contents.push_str("addnode=\n")
        } else {
            for peer in &config.initial_peers {
                contents.push_str(&format!("addnode={}\n", peer))
            }
        }

        contents
    }
}
