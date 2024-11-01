// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use anyhow::Result;
use std::sync::Mutex;
use std::sync::MutexGuard;
use sui_config::NodeConfig;
use sui_node::SuiNodeHandle;
use sui_types::base_types::AuthorityName;
use sui_types::base_types::ConciseableName;
use sui_types::crypto::KeypairTraits;
use tap::TapFallible;
use tracing::{error, info};

use super::container::Container;

/// A handle to an in-memory Sui Node.
///
/// Each Node is attempted to run in isolation from each other by running them in their own tokio
/// runtime in a separate thread. By doing this we can ensure that all asynchronous tasks
/// associated with a Node are able to be stopped when desired (either when a Node is dropped or
/// explicitly stopped by calling [`Node::stop`]) by simply dropping that Node's runtime.
#[derive(Debug)]
pub struct Node {
    container: Mutex<Option<Container>>,
    config: Mutex<NodeConfig>,
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
            container: Default::default(),
            config: config.into(),
            runtime_type: RuntimeType::SingleThreaded,
        }
    }

    /// Return the `name` of this Node
    pub fn name(&self) -> AuthorityName {
        self.config().protocol_public_key()
    }

    pub fn config(&self) -> MutexGuard<'_, NodeConfig> {
        self.config.lock().unwrap()
    }

    pub fn json_rpc_address(&self) -> std::net::SocketAddr {
        self.config().json_rpc_address
    }

    /// Start this Node
    pub async fn spawn(&self) -> Result<()> {
        info!(name =% self.name().concise(), "starting in-memory node");
        let config = self.config().clone();
        *self.container.lock().unwrap() = Some(Container::spawn(config, self.runtime_type).await);
        Ok(())
    }

    /// Start this Node, waiting until its completely started up.
    pub async fn start(&self) -> Result<()> {
        self.spawn().await
    }

    /// Stop this Node
    pub fn stop(&self) {
        info!(name =% self.name().concise(), "stopping in-memory node");
        *self.container.lock().unwrap() = None;
        info!(name =% self.name().concise(), "node stopped");
    }

    /// If this Node is currently running
    pub fn is_running(&self) -> bool {
        self.container
            .lock()
            .unwrap()
            .as_ref()
            .map_or(false, |c| c.is_alive())
    }

    pub fn get_node_handle(&self) -> Option<SuiNodeHandle> {
        self.container
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|c| c.get_node_handle())
    }

    /// Perform a health check on this Node by:
    /// * Checking that the node is running
    /// * Calling the Node's gRPC Health service if it's a validator.
    pub async fn health_check(&self, is_validator: bool) -> Result<(), HealthCheckError> {
        {
            let lock = self.container.lock().unwrap();
            let container = lock.as_ref().ok_or(HealthCheckError::NotRunning)?;
            if !container.is_alive() {
                return Err(HealthCheckError::NotRunning);
            }
        }

        if is_validator {
            let network_address = self.config().network_address().clone();
            let tls_config = sui_tls::create_rustls_client_config(
                self.config().network_key_pair().public().to_owned(),
                sui_tls::SUI_VALIDATOR_SERVER_NAME.to_string(),
                None,
            );
            let channel = mysten_network::client::connect(&network_address, Some(tls_config))
                .await
                .map_err(|err| anyhow!(err.to_string()))
                .map_err(HealthCheckError::Failure)
                .tap_err(|e| error!("error connecting to {}: {e}", self.name().concise()))?;

            let mut client = tonic_health::pb::health_client::HealthClient::new(channel);
            client
                .check(tonic_health::pb::HealthCheckRequest::default())
                .await
                .map_err(|e| HealthCheckError::Failure(e.into()))
                .tap_err(|e| {
                    error!(
                        "error performing health check on {}: {e}",
                        self.name().concise()
                    )
                })?;
        }

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
        let swarm = Swarm::builder().build();

        let validator = swarm.validator_nodes().next().unwrap();

        validator.start().await.unwrap();
        validator.health_check(true).await.unwrap();
        validator.stop();
        validator.health_check(true).await.unwrap_err();

        validator.start().await.unwrap();
        validator.health_check(true).await.unwrap();
    }
}
