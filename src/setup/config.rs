use serde::{Deserialize, Serialize};

use std::{
    collections::HashSet,
    env,
    ffi::OsString,
    fs, io,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
};

use crate::setup::node::Action;

const CONFIG: &str = "config.toml";
const DEFAULT_PORT: u16 = 8080;

/// Convenience struct for reading Ziggurat's configuration file.
#[derive(Deserialize)]
struct ConfigFile {
    kind: NodeKind,
    path: PathBuf,
    start_command: String,
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
        // Set the port explicitly.
        let mut local_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        local_addr.set_port(DEFAULT_PORT);

        Self {
            local_addr,
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
pub(super) struct NodeMetaData {
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
    pub(super) fn new() -> io::Result<Self> {
        let path = &env::current_dir()?.join(CONFIG);
        let config_string = fs::read_to_string(path)?;
        let config_file: ConfigFile = toml::from_str(&config_string)?;

        let args_from = |command: &str| -> Vec<OsString> {
            command.split_whitespace().map(OsString::from).collect()
        };

        let mut start_args = args_from(&config_file.start_command);
        let start_command = start_args.remove(0);

        Ok(Self {
            kind: config_file.kind,
            path: config_file.path,
            start_command,
            start_args,
        })
    }
}

/// Convenience struct for writing a zebra compatible configuration file.
#[derive(Serialize)]
pub(super) struct ZebraConfigFile {
    network: NetworkConfig,
    state: StateConfig,
    tracing: TracingConfig,
}

impl ZebraConfigFile {
    /// Generate the toml configuration as a string.
    pub(super) fn generate(config: &NodeConfig) -> Result<String, toml::ser::Error> {
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
        toml::to_string(&zebra_config)
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
