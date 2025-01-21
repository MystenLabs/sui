use std::{future::Future, pin::Pin};

use sui_config::NodeConfig;
use sui_exex::ExExContext;

pub struct NodeBuilder {
    config: NodeConfig,
    exex: Box<dyn FnOnce(ExExContext) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> + Send + Sync>,
}

impl NodeBuilder {
    pub fn new() -> Self
    {
        NodeBuilder {
            config : todo!(),
            exex: todo!(),
        }
    }

    pub fn with_config(mut self, config: NodeConfig) -> Self {
        self.config = config;
        self
    }

    pub fn with_exex<F, Fut>(mut self, exex: F) -> Self 
    where
        F: FnOnce(ExExContext) -> Fut + Send + Sync + 'static,
        Fut: futures::Future<Output = anyhow::Result<()>> + Send + 'static,
    {
        self.exex = Box::new(move |ctx| Box::pin(exex(ctx)));
        self
    }

    pub async fn run(&self) {
        todo!();
    }
}
