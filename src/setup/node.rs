use crate::{
    helpers::respond_to_handshake,
    protocol::{
        message::{Filter, MessageFilter},
        payload::{
            block::{Block, Headers},
            Hash, Inv, Nonce,
        },
    },
    setup::config::{NodeConfig, NodeKind, NodeMetaData, ZcashdConfigFile, ZebraConfigFile},
};

use tokio::{
    net::TcpListener,
    process::{Child, Command},
};

use std::{fs, net::SocketAddr, process::Stdio};

const ZEBRA_CONFIG: &str = "zebra.toml";
const ZCASHD_CONFIG: &str = "zcash.conf";

pub enum Action {
    /// Performs no action
    None,
    /// Waits for the node to connect at the addr, connection is then terminated.
    /// This is useful for indicating that the node has started and is available for
    /// other connections.
    WaitForConnection(SocketAddr),
    /// Seeds the node with `block_count` blocks from the testnet chain, by connecting from a socket
    /// on `socket_addr` and sending the appropriate data. After this, the connection is terminated.
    ///
    /// **Warning**: this currently only works for zcashd type nodes, for zebra the behaviour
    /// is equivalent to WaitForConnection.
    SeedWithTestnetBlocks {
        /// The socket address to use when connecting to the node
        socket_addr: SocketAddr,
        /// The number of initial testnet blocks to seed. Note that this is capped by the number of blocks available
        /// from [Block::initial_testnet_blocks].
        block_count: usize,
    },
}

/// Represents an instance of a node, its configuration and setup/teardown intricacies.
pub struct Node {
    /// Configuration definable in tests and written to the node's configuration file on start.
    config: NodeConfig,
    /// Type, path to binary, various commands for starting, stopping, cleanup, network
    /// configuration.
    meta: NodeMetaData,
    /// Process of the running node.
    process: Option<Child>,
}

impl Default for Node {
    /// Creates a new [`Node`] instance.
    ///
    /// Once created, it can be configured with calls to [`initial_peers`], [`max_peers`] and [`log_to_stdout`].
    ///
    /// [`Node`]: struct@Node
    /// [`NodeMetaData`]: struct@crate::setup::config::NodeMetaData
    /// [`initial_peers`]: method@Node::initial_peers
    /// [`max_peers`]: method@Node::max_peers
    /// [`log_to_stdout`]: method@Node::log_to_stdout
    fn default() -> Self {
        // Config (to be written to node configuration file).
        let config = NodeConfig::new();
        let meta = NodeMetaData::new();

        Self {
            config,
            meta,
            process: None,
        }
    }
}

impl Node {
    /// Returns the (external) address of the node.
    pub fn addr(&self) -> SocketAddr {
        self.config.local_addr
    }

    /// Sets the initial peers (ports only) for the node.
    ///
    /// The ip used to construct the addresses can be optionally set in the configuration file and
    /// otherwise defaults to localhost.
    pub fn initial_peers(&mut self, peers: Vec<SocketAddr>) -> &mut Self {
        self.config.initial_peers = peers.iter().map(|addr| format!("{}", addr)).collect();

        self
    }

    /// Sets the maximum connection value for the node.
    pub fn max_peers(&mut self, max_peers: usize) -> &mut Self {
        self.config.max_peers = max_peers;
        self
    }

    /// Sets whether to log the node's output to Ziggurat's output stream.
    pub fn log_to_stdout(&mut self, log_to_stdout: bool) -> &mut Self {
        self.config.log_to_stdout = log_to_stdout;
        self
    }

    /// Sets the initial action to undertake once the node has started. See [Action] for more
    /// information on what the actions pertain.
    pub fn initial_action(&mut self, action: Action) -> &mut Self {
        self.config.initial_action = action;
        self
    }

    /// Starts the node instance.
    ///
    /// This function will write the appropriate configuration file and run the start command
    /// provided in `config.toml`.
    pub async fn start(&mut self) {
        // cleanup any previous runs (node.stop won't always be reached e.g. test panics, or SIGINT)
        self.cleanup();

        // Setup the listener if there is some initial action required
        let listener = match self.config.initial_action {
            Action::None => None,
            Action::WaitForConnection(addr)
            | Action::SeedWithTestnetBlocks {
                socket_addr: addr,
                block_count: _,
            } => {
                let bound_listener = TcpListener::bind(addr).await.unwrap();
                self.config
                    .initial_peers
                    .insert(format!("{}", bound_listener.local_addr().unwrap()));
                Some(bound_listener)
            }
        };

        // Generate config files for Zebra or Zcashd node.
        self.generate_config_file();

        let (stdout, stderr) = match self.config.log_to_stdout {
            true => (Stdio::inherit(), Stdio::inherit()),
            false => (Stdio::null(), Stdio::null()),
        };

        let process = Command::new(&self.meta.start_command)
            .current_dir(&self.meta.path)
            .args(&self.meta.start_args)
            .stdin(Stdio::null())
            .stdout(stdout)
            .stderr(stderr)
            .kill_on_drop(true)
            .spawn()
            .expect("node failed to start");

        self.process = Some(process);

        self.perform_initial_action(listener).await;
    }

    async fn perform_initial_action(&self, listener: Option<TcpListener>) {
        match self.config.initial_action {
            Action::None => {}
            Action::WaitForConnection(_) => {
                listener.unwrap().accept().await.unwrap();
            }
            Action::SeedWithTestnetBlocks {
                socket_addr: _,
                block_count: _,
            } if self.meta.kind == NodeKind::Zebra => {
                // not supported for zebra yet, so we just wait for connection at least
                listener.unwrap().accept().await.unwrap();
            }
            Action::SeedWithTestnetBlocks {
                socket_addr: _,
                block_count,
            } => {
                use crate::protocol::message::Message;
                let mut stream = respond_to_handshake(listener.unwrap()).await.unwrap();

                let filter = MessageFilter::with_all_auto_reply()
                    .with_getheaders_filter(Filter::Disabled)
                    .with_getdata_filter(Filter::Disabled);

                let genesis_block = Block::testnet_genesis();
                // initial blocks, skipping genesis as it doesn't get sent
                let blocks = Block::initial_testnet_blocks()
                    .into_iter()
                    .take(block_count)
                    .skip(1)
                    .collect::<Vec<_>>();

                // respond to GetHeaders(Block[0])
                match filter.read_from_stream(&mut stream).await.unwrap() {
                    Message::GetHeaders(locations) => {
                        // The request should be from the genesis hash onwards,
                        // i.e. locator_hash = [genesis.hash], stop_hash = [0]
                        assert_eq!(
                            locations.block_locator_hashes,
                            vec![genesis_block.double_sha256().unwrap()]
                        );
                        assert_eq!(locations.hash_stop, Hash::zeroed());

                        // Reply with headers for the initial block headers
                        let headers = blocks.iter().map(|block| block.header.clone()).collect();
                        Message::Headers(Headers::new(headers))
                            .write_to_stream(&mut stream)
                            .await
                            .unwrap();
                    }
                    msg => panic!("Expected GetHeaders but got: {:?}", msg),
                }

                // respond to GetData(inv) for the initial blocks
                match filter.read_from_stream(&mut stream).await.unwrap() {
                    Message::GetData(inv) => {
                        // The request must be for the initial blocks
                        let inv_hashes = blocks.iter().map(|block| block.inv_hash()).collect();
                        let expected = Inv::new(inv_hashes);
                        assert_eq!(inv, expected);

                        // Send the blocks
                        for block in blocks {
                            Message::Block(Box::new(block))
                                .write_to_stream(&mut stream)
                                .await
                                .unwrap();
                        }
                    }
                    msg => panic!("Expected GetData but got: {:?}", msg),
                }

                Message::Ping(Nonce::default())
                    .write_to_stream(&mut stream)
                    .await
                    .unwrap();
                let filter =
                    MessageFilter::with_all_auto_reply().with_ping_filter(Filter::Disabled);
                match filter.read_from_stream(&mut stream).await.unwrap() {
                    Message::Pong(_) => {}
                    msg => panic!("Expected Pong but got: {:?}", msg),
                }
            }
        }
    }

    /// Stops the node instance.
    ///
    /// The stop command will only be run if provided in the `config.toml` file as it may not be
    /// necessary to shutdown a node (killing the process is sometimes sufficient).
    pub async fn stop(&mut self) {
        let mut child = self.process.take().unwrap();

        // Stop node process, and check for crash
        // (needs to happen before cleanup)
        let crashed = match child.try_wait().unwrap() {
            None => {
                child.kill().await.unwrap();
                None
            }
            Some(exit_code) if exit_code.success() => {
                Some("but exited successfully somehow".to_string())
            }
            Some(exit_code) => Some(format!("crashed with {}", exit_code)),
        };

        self.cleanup();

        if let Some(crash_msg) = crashed {
            panic!("Node exited early, {}", crash_msg);
        }
    }

    fn generate_config_file(&self) {
        let path = self.config_filepath();
        let content = match self.meta.kind {
            NodeKind::Zebra => ZebraConfigFile::generate(&self.config),
            NodeKind::Zcashd => ZcashdConfigFile::generate(&self.config),
        };

        fs::write(path, content).unwrap();
    }

    fn config_filepath(&self) -> std::path::PathBuf {
        match self.meta.kind {
            NodeKind::Zebra => self.meta.path.join(ZEBRA_CONFIG),
            NodeKind::Zcashd => self.meta.path.join(ZCASHD_CONFIG),
        }
    }

    fn cleanup(&self) {
        self.cleanup_config_file();
        self.cleanup_cache();
    }

    fn cleanup_config_file(&self) {
        let path = self.config_filepath();
        match std::fs::remove_file(path) {
            // File may not exist, so we let that error through
            Err(err) if err.kind() != std::io::ErrorKind::NotFound => {
                panic!("Error removing config file: {}", err)
            }
            _ => {}
        }
    }

    fn cleanup_cache(&self) {
        // No cache for zebra as it is configured in ephemeral mode
        if let NodeKind::Zcashd = self.meta.kind {
            // Default cache location is ~/.zcash
            let path = home::home_dir().unwrap().join(".zcash");

            match std::fs::remove_dir_all(path) {
                // Directory may not exist, so we let that error through
                Err(err) if err.kind() != std::io::ErrorKind::NotFound => {
                    panic!("Error cleaning up zcashd cache: {}", err)
                }
                _ => {}
            }
        }
    }
}
