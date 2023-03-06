use std::{net::SocketAddr, sync::Arc};

use jsonrpsee::server::{RpcModule, ServerBuilder, ServerHandle};
use parking_lot::Mutex;
use tracing::debug;
use ziggurat_core_crawler::summary::NetworkSummary;

pub struct RpcContext(Arc<Mutex<NetworkSummary>>);

/// Make this now 200MB
pub const MAX_RESPONSE_SIZE: u32 = 200000000;

impl RpcContext {
    /// Creates a new RpcContext.
    pub fn new(known_network: Arc<Mutex<NetworkSummary>>) -> RpcContext {
        RpcContext(known_network)
    }
}

impl std::ops::Deref for RpcContext {
    type Target = Mutex<NetworkSummary>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub async fn initialize_rpc_server(rpc_addr: SocketAddr, rpc_context: RpcContext) -> ServerHandle {
    let server = ServerBuilder::default()
        .max_response_body_size(MAX_RESPONSE_SIZE)
        .build(rpc_addr)
        .await
        .unwrap();
    let module = create_rpc_module(rpc_context);

    debug!("Starting RPC server at {:?}", server.local_addr().unwrap());
    let server_handle = server.start(module).unwrap();

    debug!("RPC server was successfully started");
    server_handle
}

fn create_rpc_module(rpc_context: RpcContext) -> RpcModule<RpcContext> {
    let mut module = RpcModule::new(rpc_context);

    module
        .register_method("getmetrics", |_, rpc_context| {
            Ok(rpc_context.lock().clone())
        })
        .unwrap();

    module
}
