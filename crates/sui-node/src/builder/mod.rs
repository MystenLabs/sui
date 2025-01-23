use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use std::{future::Future, pin::Pin};

use crate::SuiNode;
use mysten_common::sync::async_once_cell::AsyncOnceCell;
use sui_config::Config;
use sui_config::NodeConfig;
use sui_core::runtime::SuiRuntimes;
use sui_exex::ExExContext;
use sui_types::supported_protocol_versions::SupportedProtocolVersions;
use tokio::sync::broadcast;
use tracing::{error, info};

pub struct NodeBuilder {
    config: Option<NodeConfig>,
    exex: Option<
        Vec<
            Box<
                dyn FnOnce(ExExContext) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>
                    + Send
                    + Sync,
            >,
        >,
    >,
}

impl NodeBuilder {
    pub fn new() -> Self {
        NodeBuilder {
            config: None,
            exex: None,
        }
    }

    pub fn with_config(mut self, config_path: PathBuf) -> Self {
        let mut cfg = NodeConfig::load(config_path).unwrap();
        assert!(
            cfg.supported_protocol_versions.is_none(),
            "supported_protocol_versions cannot be read from the config file"
        );
        cfg.supported_protocol_versions = Some(SupportedProtocolVersions::SYSTEM_DEFAULT);

        self.config = Some(cfg);

        self
    }

    pub fn with_exex<F, Fut>(mut self, exex: F) -> Self
    where
        F: FnOnce(ExExContext) -> Fut + Send + Sync + 'static,
        Fut: futures::Future<Output = anyhow::Result<()>> + Send + 'static,
    {
        self.exex = Some(vec![Box::new(move |ctx| Box::pin(exex(ctx)))]);
        self
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        info!("Starting Node...");

        let cfg = self
            .config
            .clone()
            .expect("NodeBuilder : Config was not provided");
        let runtimes = SuiRuntimes::new(&cfg);
        let rpc_runtime = runtimes.json_rpc.handle().clone();
        let registry_service = mysten_metrics::start_prometheus_server(cfg.metrics_address);

        // Run node in a separate runtime so that admin/monitoring functions continue to work
        // if it deadlocks.
        let node_once_cell = Arc::new(AsyncOnceCell::<Arc<SuiNode>>::new());
        let node_once_cell_clone = node_once_cell.clone();

        let (runtime_shutdown_tx, runtime_shutdown_rx) = broadcast::channel::<()>(1);

        runtimes.sui_node.spawn(async move {
            match SuiNode::start_async(cfg, registry_service, Some(rpc_runtime), "0.0.1").await {
                Ok(sui_node) => node_once_cell_clone
                    .set(sui_node)
                    .expect("Failed to set node in AsyncOnceCell"),
    
                Err(e) => {
                    error!("Failed to start node: {e:?}");
                    std::process::exit(1);
                }
            }
    
            // get node, subscribe to shutdown channel
            let node = node_once_cell_clone.get().await;
            let mut shutdown_rx = node.subscribe_to_shutdown_channel();
    
            // when we get a shutdown signal from sui-node, forward it on to the runtime_shutdown_channel here in
            // main to signal runtimes to all shutdown.
            tokio::select! {
               _ = shutdown_rx.recv() => {
                    runtime_shutdown_tx.send(()).expect("failed to forward shutdown signal from sui-node to sui-node main");
                }
            }
            // TODO: Do we want to provide a way for the node to gracefully shutdown?
            loop {
                tokio::time::sleep(Duration::from_secs(1000)).await;
            }
        });

        Ok(())
    }
}

#[cfg(not(unix))]
async fn wait_termination(mut shutdown_rx: tokio::sync::broadcast::Receiver<()>) {
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {},
        _ = shutdown_rx.recv() => {},
    }
}

#[cfg(unix)]
async fn wait_termination(mut shutdown_rx: tokio::sync::broadcast::Receiver<()>) {
    use futures::FutureExt;
    use tokio::signal::unix::*;

    let sigint = tokio::signal::ctrl_c().boxed();
    let mut sigterm = signal(SignalKind::terminate()).unwrap();
    let sigterm_recv = sigterm.recv().boxed();
    let shutdown_recv = shutdown_rx.recv().boxed();

    tokio::select! {
        _ = sigint => {},
        _ = sigterm_recv => {},
        _ = shutdown_recv => {},
    }
}
