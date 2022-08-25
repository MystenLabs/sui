// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use anyhow::Result;
use futures::FutureExt;
use std::thread;
use sui_config::NodeConfig;
use sui_node::SuiNode;
use sui_types::base_types::SuiAddress;
use tap::TapFallible;
use tracing::{error, trace};

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

#[derive(Debug)]
struct Container {
    thread: Option<thread::JoinHandle<()>>,
    cancel_sender: Option<tokio::sync::oneshot::Sender<()>>,
}

/// When dropped, stop and wait for the node running in this Container to completely shutdown.
impl Drop for Container {
    fn drop(&mut self) {
        trace!("dropping Container");

        let thread = self.thread.take().unwrap();
        let cancel_handle = self.cancel_sender.take().unwrap();

        // Notify the thread to shutdown
        let _ = cancel_handle.send(());

        // Wait for the thread to join
        thread.join().unwrap();

        trace!("finished dropping Container");
    }
}

impl Container {
    /// Spawn a new Node.
    pub fn spawn(
        config: NodeConfig,
        runtime: RuntimeType,
    ) -> (tokio::sync::oneshot::Receiver<()>, Self) {
        let (startup_sender, startup_reciever) = tokio::sync::oneshot::channel();
        let (cancel_sender, cancel_reciever) = tokio::sync::oneshot::channel();

        let thread = thread::spawn(move || {
            let span = tracing::span!(
                tracing::Level::INFO,
                "node",
                name =% config.sui_address()
            );
            let _guard = span.enter();

            let mut builder = match runtime {
                RuntimeType::SingleThreaded => tokio::runtime::Builder::new_current_thread(),
                RuntimeType::MultiThreaded => {
                    thread_local! {
                        static SPAN: std::cell::RefCell<Option<tracing::span::EnteredSpan>> =
                            std::cell::RefCell::new(None);
                    }
                    let mut builder = tokio::runtime::Builder::new_multi_thread();
                    let span = span.clone();
                    builder
                        .on_thread_start(move || {
                            SPAN.with(|maybe_entered_span| {
                                *maybe_entered_span.borrow_mut() = Some(span.clone().entered());
                            });
                        })
                        .on_thread_stop(|| {
                            SPAN.with(|maybe_entered_span| {
                                maybe_entered_span.borrow_mut().take();
                            });
                        });

                    builder
                }
            };
            let runtime = builder.enable_all().build().unwrap();

            runtime.block_on(async move {
                let _server = SuiNode::start(&config).await.unwrap();
                // Notify that we've successfully started the node
                let _ = startup_sender.send(());
                // run until canceled
                cancel_reciever.map(|_| ()).await;

                trace!("cancellation received; shutting down thread");
            });
        });

        (
            startup_reciever,
            Self {
                thread: Some(thread),
                cancel_sender: Some(cancel_sender),
            },
        )
    }

    /// Check to see that the Node is still alive by checking if the receiving side of the
    /// `cancel_sender` has been dropped.
    ///
    //TODO When we move to rust 1.61 we should also use
    // https://doc.rust-lang.org/stable/std/thread/struct.JoinHandle.html#method.is_finished
    // in order to check if the thread has finished.
    pub fn is_alive(&self) -> bool {
        if let Some(cancel_sender) = &self.cancel_sender {
            !cancel_sender.is_closed()
        } else {
            false
        }
    }
}

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
