use std::path::PathBuf;
use std::{future::Future, pin::Pin};

use sui_config::Config;
use sui_config::NodeConfig;
use sui_exex::ExExContext;

pub struct NodeBuilder {
    config: Option<NodeConfig>,
    exex: Option<Box<dyn FnOnce(ExExContext) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> + Send + Sync>>,
}

impl NodeBuilder {
    pub fn new() -> Self
    {
        NodeBuilder {
            config : None,
            exex: None,
        }
    }

    pub fn with_config(mut self, config_path: PathBuf) -> Self {
        self.config = Some(NodeConfig::load(config_path).unwrap());
        self
    }

    pub fn with_exex<F, Fut>(mut self, exex: F) -> Self 
    where
        F: FnOnce(ExExContext) -> Fut + Send + Sync + 'static,
        Fut: futures::Future<Output = anyhow::Result<()>> + Send + 'static,
    {
        self.exex = Some(Box::new(move |ctx| Box::pin(exex(ctx))));
        self
    }

    pub async fn run(&self) {
        
        todo!();
    }
}
