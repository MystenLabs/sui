// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Building blocks for the experimental `sui-fork` tool.

pub mod args;
#[doc(hidden)]
pub mod cli;
pub(crate) mod context;
mod gql;
pub(crate) mod ingestion;
pub(crate) mod local_store;
pub(crate) mod metadata;
mod node;
pub(crate) mod pending;
mod proto;
pub(crate) mod remote;
mod rpc;
pub(crate) mod seed;
pub(crate) mod services;
mod startup;
pub(crate) mod store;
#[cfg(test)]
#[path = "tests/support.rs"]
mod test_support;

pub use args::DEFAULT_RPC_ADDR;
pub use args::StartArgs;
pub use node::Node;
pub use proto::forking::AdvanceCheckpointRequest;
pub use proto::forking::AdvanceCheckpointResponse;
pub use proto::forking::AdvanceClockRequest;
pub use proto::forking::AdvanceClockResponse;
pub use proto::forking::GetStatusRequest;
pub use proto::forking::GetStatusResponse;
pub use proto::forking::forking_service_client::ForkingServiceClient;

use std::net::SocketAddr;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use anyhow::anyhow;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use sui_futures::service::GRACE;
use sui_futures::service::Service;

use crate::context::Context;
use crate::seed::SeedInput;
use crate::startup::ForkParts;

/// A running forked network, served over gRPC and administered in-process.
///
/// Dropping the node aborts its tasks immediately. Deliberate exits go through [`Self::stop`] or
/// through the [`Service`] returned by [`Self::into_service`].
pub struct ForkNode {
    context: Arc<Context>,
    service: Service,
    rpc_address: SocketAddr,
    data_dir: PathBuf,
    resumed: bool,
}

impl ForkNode {
    /// Start a forked network and return once its RPC listener accepts connections.
    ///
    /// The listener binds before anything durable is touched, so a taken port fails before a data
    /// directory exists. Fork state already under the data directory is resumed when it matches
    /// the requested network and checkpoint. Returns an error when the listener cannot bind, the
    /// live network cannot be reached, or the data directory describes a different fork.
    pub async fn start(args: StartArgs) -> Result<Self> {
        let StartArgs {
            network,
            checkpoint,
            data_dir,
            addresses,
            object_ids,
            rpc_addr,
            version,
        } = args;
        let seed_input = SeedInput {
            addresses: addresses.into_iter().collect(),
            object_ids: object_ids.into_iter().collect(),
        };

        let listener = startup::bind(rpc_addr).await?;
        let parts = startup::initialize(network, checkpoint, version, data_dir, seed_input).await?;
        Self::from_parts(parts, listener, version).await
    }

    /// Serve initialized fork parts over `listener`.
    pub(crate) async fn from_parts(
        parts: ForkParts,
        listener: TcpListener,
        version: &'static str,
    ) -> Result<Self> {
        let ForkParts {
            context,
            subscription_handle,
            indexer_service,
            data_dir,
            resumed,
        } = parts;
        let context = Arc::new(context);
        let (rpc_address, rpc_service) =
            startup::serve(context.clone(), subscription_handle, listener, version).await?;

        Ok(Self {
            context,
            service: run_services(rpc_service, indexer_service),
            rpc_address,
            data_dir,
            resumed,
        })
    }

    /// Return the address of the bound RPC listener.
    pub fn rpc_address(&self) -> SocketAddr {
        self.rpc_address
    }

    /// Return the directory holding the fork's persistent state, which a later start resumes from.
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Return whether this start resumed previously persisted fork state.
    pub fn resumed(&self) -> bool {
        self.resumed
    }

    /// Advance the clock by `duration` and seal the clock transaction into a new checkpoint.
    ///
    /// Same contract as the forking gRPC service's `AdvanceClock`, and returns once the checkpoint
    /// is indexed.
    pub async fn advance_clock(&self, duration: Duration) -> AdvanceClockResponse {
        self.context.advance_clock(duration).await
    }

    /// Seal a new checkpoint without executing another transaction.
    ///
    /// Same contract as the forking gRPC service's `AdvanceCheckpoint`, and returns once the
    /// checkpoint is indexed.
    pub async fn advance_checkpoint(&self) -> AdvanceCheckpointResponse {
        self.context.advance_checkpoint().await
    }

    /// Report the fork's epoch, checkpoint tip, clock, and fork point.
    pub async fn status(&self) -> GetStatusResponse {
        self.context.status().await
    }

    /// Stop the fork gracefully, draining RPC requests before stopping the embedded indexer.
    ///
    /// Everything sealed is already durable, so nothing is lost. The drain waits for open
    /// subscription streams, which end only once the indexer closes, so the stop is bounded by
    /// [`GRACE`] and reports an error after aborting whatever remains.
    pub async fn stop(self) -> Result<()> {
        match tokio::time::timeout(GRACE, self.service.shutdown()).await {
            Ok(result) => Ok(result?),
            Err(_) => Err(anyhow!(
                "timed out stopping the forked network after {GRACE:?}"
            )),
        }
    }

    /// Convert the running fork into a [`Service`] for composition with other services.
    ///
    /// In-process administration ends with the conversion, while the forking gRPC service stays
    /// reachable at [`Self::rpc_address`].
    pub fn into_service(self) -> Service {
        self.service
    }
}

/// Run the RPC server and the embedded indexer as one service that stops in order.
///
/// Shutdown drains RPC requests before stopping the indexer, because a draining request may still
/// be waiting for its checkpoint to be indexed. Either task stopping on its own is a failure.
fn run_services(mut rpc_service: Service, mut indexer_service: Service) -> Service {
    let (stop_sender, stop_receiver) = oneshot::channel::<()>();

    Service::new()
        .with_shutdown_signal(async move {
            let _ = stop_sender.send(());
        })
        .spawn(async move {
            enum Exit {
                Stop,
                Rpc(Result<()>),
                Indexer(Result<()>),
            }

            let exit = tokio::select! {
                _ = stop_receiver => Exit::Stop,
                result = rpc_service.join() => Exit::Rpc(result),
                result = indexer_service.join() => Exit::Indexer(result),
            };

            match exit {
                Exit::Stop => {
                    rpc_service.shutdown().await?;
                    indexer_service.shutdown().await?;
                    Ok(())
                }
                Exit::Rpc(Ok(())) => Err(anyhow!("rpc server stopped unexpectedly")),
                Exit::Rpc(Err(error)) => Err(error.context("rpc server failed")),
                Exit::Indexer(Ok(())) => {
                    Err(anyhow!("embedded rpc-store indexer stopped unexpectedly"))
                }
                Exit::Indexer(Err(error)) => {
                    Err(error.context("embedded rpc-store indexer failed"))
                }
            }
        })
}
