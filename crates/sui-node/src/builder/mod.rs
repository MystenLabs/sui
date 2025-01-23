use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::SuiNode;
use sui_config::Config;
use sui_config::NodeConfig;
use sui_core::runtime::SuiRuntimes;
use sui_types::supported_protocol_versions::SupportedProtocolVersions;
use tracing::{debug, info};

pub struct NodeBuilder {
    config: Option<NodeConfig>,
}

impl NodeBuilder {
    pub fn new() -> Self {
        debug!("Creating new NodeBuilder");
        NodeBuilder { config: None }
    }

    pub fn with_config(mut self, config_path: PathBuf) -> Self {
        info!("Loading config from {:?}", config_path);
        let mut cfg = NodeConfig::load(config_path).unwrap();
        assert!(
            cfg.supported_protocol_versions.is_none(),
            "supported_protocol_versions cannot be read from the config file"
        );
        cfg.supported_protocol_versions = Some(SupportedProtocolVersions::SYSTEM_DEFAULT);
        debug!("Setting supported protocol versions to system default");

        self.config = Some(cfg);
        debug!("Config loaded successfully");

        self
    }

    pub async fn run(&self) -> anyhow::Result<(Arc<SuiNode>, Arc<SuiRuntimes>)> {
        info!("Starting Node...");
        let cfg = self
            .config
            .as_ref()
            .expect("NodeBuilder: Config was not provided");

        debug!("Initializing SuiRuntimes");
        let runtimes = Arc::new(SuiRuntimes::new(&cfg));
        let rpc_runtime = runtimes.json_rpc.handle().clone();

        debug!("Starting Prometheus metrics server");
        let registry_service = mysten_metrics::start_prometheus_server(cfg.metrics_address);

        info!("Starting SuiNode");
        let node = SuiNode::start(cfg.clone(), registry_service, Some(rpc_runtime)).await?;
        info!("SuiNode started successfully");

        Ok((node, runtimes))
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
