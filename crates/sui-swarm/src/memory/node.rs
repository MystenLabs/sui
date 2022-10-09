// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use anyhow::Result;
use sui_config::NodeConfig;
use sui_types::base_types::SuiAddress;
use tap::TapFallible;
use tracing::{error, trace};

use super::container::Container;

/// A handle to an in-memory Sui Node.
///
/// Each Node is attempted to run in isolation from each other by running them in their own tokio
/// runtime in a separate thread. By doing this we can ensure that all asynchronous tasks
/// associated with a Node are able to be stopped when desired (either when a Node is dropped or
/// explicitly stopped by calling [`Node::stop`]) by simply dropping that Node's runtime.
#[derive(Debug)]
pub struct Node {
    thread: Option<Container>,
    config: NodeConfig,
    runtime_type: RuntimeType,
}

impl Node {
    /// Create a new Node from the provided `NodeConfig`.
    ///
    /// The Node is returned without being started. See [`Node::spawn`] or [`Node::start`] for how to
    /// start the node.
    ///
    /// [`NodeConfig`]: sui_config::NodeConfig
    pub fn new(config: NodeConfig) -> Self {
        Self {
            thread: None,
            config,
            runtime_type: RuntimeType::SingleThreaded,
        }
    }

    /// Return the `name` of this Node
    pub fn name(&self) -> SuiAddress {
        self.config.sui_address()
    }

    pub fn json_rpc_address(&self) -> std::net::SocketAddr {
        self.config.json_rpc_address
    }

    /// Start this Node, returning a handle that will resolve when the node has completed starting
    /// up.
    pub fn spawn(&mut self) -> Result<tokio::sync::oneshot::Receiver<()>> {
        trace!(name =% self.name(), "starting in-memory node");
        let (startup_reciever, node_handle) =
            Container::spawn(self.config.clone(), self.runtime_type);
        self.thread = Some(node_handle);
        Ok(startup_reciever)
    }

    /// Start this Node, waiting until its completely started up.
    pub async fn start(&mut self) -> Result<()> {
        let startup_reciever = self.spawn()?;
        startup_reciever.await?;
        Ok(())
    }

    /// Stop this Node
    pub fn stop(&mut self) {
        trace!(name =% self.name(), "stopping in-memory node");
        self.thread = None;
    }

    /// Perform a health check on this Node by:
    /// * Checking that the node is running
    /// * Calling the Node's gRPC Health service
    pub async fn health_check(&self) -> Result<(), HealthCheckError> {
        let thread = self.thread.as_ref().ok_or(HealthCheckError::NotRunning)?;
        if !thread.is_alive() {
            return Err(HealthCheckError::NotRunning);
        }

        let channel = mysten_network::client::connect(self.config.network_address())
            .await
            .map_err(|err| anyhow!(err.to_string()))
            .map_err(HealthCheckError::Failure)
            .tap_err(|e| error!("error connecting to {}: {e}", self.name()))?;
        let mut client = tonic_health::proto::health_client::HealthClient::new(channel);
        client
            .check(tonic_health::proto::HealthCheckRequest::default())
            .await
            .map_err(|e| HealthCheckError::Failure(e.into()))
            .tap_err(|e| error!("error performing health check on {}: {e}", self.name()))?;

        Ok(())
    }
}

#[derive(Debug)]
pub enum HealthCheckError {
    NotRunning,
    Failure(anyhow::Error),
    Unknown(anyhow::Error),
}

impl std::fmt::Display for HealthCheckError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for HealthCheckError {}

/// The type of tokio runtime that should be used for a particular Node
#[derive(Clone, Copy, Debug)]
pub enum RuntimeType {
    SingleThreaded,
    MultiThreaded,
}

#[cfg(test)]
mod test {
    use crate::memory::Swarm;

    #[tokio::test]
    async fn start_and_stop() {
        telemetry_subscribers::init_for_testing();
        let mut swarm = Swarm::builder().build();

        let validator = swarm.validators_mut().next().unwrap();

        validator.start().await.unwrap();
        validator.health_check().await.unwrap();
        validator.stop();
        validator.health_check().await.unwrap_err();

        validator.start().await.unwrap();
        validator.health_check().await.unwrap();
    }
}
