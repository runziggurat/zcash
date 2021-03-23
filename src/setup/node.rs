use crate::setup::config::{NodeConfig, NodeKind, NodeMetaData, ZcashdConfigFile, ZebraConfigFile};

use tokio::process::{Child, Command};

use std::{env, fs, net::SocketAddr, process::Stdio};

pub struct Node {
    /// Configuration definable in tests and written to the node's configuration file on start.
    config: NodeConfig,
    /// Type, path to binary, various commands for starting, stopping, cleanup.
    meta: NodeMetaData,
    /// Process of the running node.
    process: Option<Child>,
}

impl Node {
    pub fn new(config: NodeConfig) -> Self {
        // 1. Configuration file read into NodeMeta.
        // 2. Node instance from Config + Meta, process is None.

        let meta = NodeMetaData::new(&env::current_dir().unwrap().join("config.toml"));

        Self {
            config,
            meta,
            process: None,
        }
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.config.local_addr
    }

    pub async fn start(&mut self) {
        // 1. Write necessary config files.
        // 2. Create and run command.

        // Generate config files for Zebra or Zcashd node.
        self.generate_config_file();

        let stdout = match self.config.log_to_stdout {
            true => Stdio::inherit(),
            false => Stdio::null(),
        };

        let process = Command::new(&self.meta.command)
            .current_dir(&self.meta.path)
            .args(&self.meta.args)
            .stdin(Stdio::null())
            .stdout(stdout)
            .kill_on_drop(true)
            .spawn()
            .expect("node failed to start");

        self.process = Some(process);

        // In future maybe ping to check if ready? Maybe in include an explicit build step here as
        // well?
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
    }

    pub async fn stop(&mut self) {
        // TODO: remove node config file?
        let mut child = self
            .process
            .take()
            .expect("process isn't currently running");
        child.kill().await.expect("failed to kill process");
    }

    fn generate_config_file(&self) {
        let (path, content) = match self.meta.kind {
            NodeKind::Zebra => (
                self.meta.path.join("node.toml"),
                ZebraConfigFile::generate(&self.config),
            ),
            NodeKind::Zcashd => (
                self.meta.path.join("zcash.conf"),
                ZcashdConfigFile::generate(&self.config),
            ),
        };

        fs::write(path, content).unwrap();
    }
}
