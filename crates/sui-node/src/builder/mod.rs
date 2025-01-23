use std::path::PathBuf;
use std::sync::Arc;

use crate::SuiNode;
use crate::SuiNodeHandle;
use sui_config::Config;
use sui_config::NodeConfig;
use sui_core::runtime::SuiRuntimes;
use sui_types::supported_protocol_versions::SupportedProtocolVersions;
use tracing::info;

pub struct NodeBuilder {
    config: Option<NodeConfig>,
}

impl NodeBuilder {
    pub fn new() -> Self {
        NodeBuilder {
            config: None,
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

    pub async fn launch(&self) -> anyhow::Result<(SuiNodeHandle, Arc<SuiRuntimes>)> {
        info!("Starting Node...");
        let cfg = self.config.as_ref().expect("NodeBuilder: Config was not provided");
        let runtimes = Arc::new(SuiRuntimes::new(&cfg));
        let rpc_runtime = runtimes.json_rpc.handle().clone();
        let registry_service = mysten_metrics::start_prometheus_server(cfg.metrics_address);

        let node = SuiNode::start(cfg.clone(), registry_service, Some(rpc_runtime)).await?;
        Ok((node.into(), runtimes))
    }
}