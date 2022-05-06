use crate::crawler::synth_node::SynthNode;

// CLI
#[derive(Debug, Parser)]
pub struct Opts {
    /// Specify the IP address and port for the node server.
    #[clap(
        long = "addr",
        short = 'a',
        parse(try_from_str),
        default_value = "0.0.0.0:8233"
    )]
    pub addr: SocketAddr,
}

/// Represents the crawler together with network metrics it has collected.
#[derive(Clone)]
pub struct Crawler {
    synth_node: SynthNode,
    pub known_network: Arc<KnownNetwork>,
}

impl Pea2Pea for Crawler {
    fn node(&self) -> &Pea2PeaNode {
        self.synth_node.node()
    }
}

impl Deref for Crawler {
    type Target = SynthNode;

    fn deref(&self) -> &Self::Target {
        &self.synth_node
    }
}

impl Crawler {
    /// Creates the crawler with the given configuration.
    pub async fn new(opts: Opts, storage: Option<StorageClient>) -> Self {
        Crawler {}
    }
}
