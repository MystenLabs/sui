// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! A fork node: a local Sui network that starts from the state of a live network at a chosen
//! checkpoint and executes transactions locally from there.
//!
//! State the fork writes itself lives in a stock `sui-rpc-store` RocksDB database, kept current by
//! an embedded indexer. State inherited from the live network is fetched lazily over GraphQL,
//! pinned at the fork checkpoint, and cached into the same database. Execution runs through
//! `simulacrum` in lock-step, so nothing advances until a transaction is executed or an admin
//! operation advances the clock or seals a checkpoint. The fork serves the standard `sui-rpc-api`
//! gRPC surface plus a forking admin service (advance-clock, advance-checkpoint, status). The
//! design is argued in this crate's `design/` directory.
//!
//! There are two ways in. Programs embed a fork through [`ForkNode::start`], which takes
//! [`StartArgs`], a version string, and a metrics registry, and returns a running [`ForkNode`].
//! The `sui-fork` binary wraps the same entry point behind [`cli::Cli`], whose client subcommands
//! drive a running fork over the forking service ([`ForkingServiceClient`]).

pub mod args;
pub mod cli;
pub(crate) mod context;
mod gql;
pub(crate) mod ingestion;
pub(crate) mod local_store;
pub(crate) mod metadata;
mod network;
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
pub use network::Network;
pub use proto::forking::AdvanceCheckpointRequest;
pub use proto::forking::AdvanceClockRequest;
pub use proto::forking::GetStatusRequest;
pub use proto::forking::forking_service_client::ForkingServiceClient;

pub(crate) use gql::GraphQLClient;

use std::net::SocketAddr;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Error;
use anyhow::Result;
use prometheus::Registry;

use simulacrum::SimulatorStore as _;
use sui_futures::service::Service;
use sui_types::base_types::ObjectID;
use sui_types::digests::TransactionDigest;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI as _;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::object::Object;
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait as _;
use sui_types::transaction::VerifiedTransaction;
use tokio::net::TcpListener;
use tracing::info;

use crate::context::Context;
use crate::seed::SeedInput;
use crate::startup::ForkParts;

// ============================================================================
// Fork node
// ============================================================================

/// A running fork node: gRPC server bound and serving, embedded indexer
/// running.
///
/// Dropping the handle aborts the fork's tasks immediately. Deliberate exits
/// go through [`Self::shutdown`], or through the [`Service`] returned by
/// [`Self::into_service`] for callers that compose the fork with other
/// services and own their own signal handling.
pub struct ForkNode {
    admin: ForkAdmin,
    service: Service,
    rpc_address: SocketAddr,
    network_name: String,
    data_dir: PathBuf,
    forked_at_checkpoint: CheckpointSequenceNumber,
    starting_checkpoint: CheckpointSequenceNumber,
    resumed: bool,
}

/// The one implementation of the fork's admin operations: [`ForkNode`]'s
/// in-process methods and the forking gRPC service both delegate here, so the
/// two surfaces cannot drift apart.
pub(crate) struct ForkAdmin {
    context: Arc<Context>,
}

/// Result of advancing the fork's clock.
#[derive(Clone, Copy, Debug)]
pub struct ClockAdvanced {
    /// Digest of the transaction that moved the on-chain clock.
    pub tx_digest: TransactionDigest,
    /// The clock's timestamp after the advance, in milliseconds.
    pub timestamp_ms: u64,
    /// The checkpoint the clock transaction was sealed into.
    pub checkpoint: CreatedCheckpoint,
}

/// A checkpoint created by the fork.
///
/// Callers only need these fields for responses and finality metadata; the
/// checkpoint itself is re-read from the store by the indexer.
#[derive(Clone, Copy, Debug)]
pub struct CreatedCheckpoint {
    /// The checkpoint's sequence number.
    pub sequence_number: CheckpointSequenceNumber,
    /// The checkpoint's timestamp, in milliseconds.
    pub timestamp_ms: u64,
}

/// Snapshot of a running fork's progress.
#[derive(Clone, Copy, Debug)]
pub struct ForkStatus {
    /// The fork's current epoch.
    pub epoch: u64,
    /// The fork's local checkpoint tip.
    pub checkpoint_sequence_number: CheckpointSequenceNumber,
    /// The fork's clock, in milliseconds.
    pub timestamp_ms: u64,
    /// The checkpoint the fork was created from.
    pub forked_at_checkpoint: CheckpointSequenceNumber,
}

impl ForkNode {
    /// Start a fork node with the given arguments, returning once its gRPC
    /// server is accepting connections.
    ///
    /// Resolves the fork point (existing local fork state when it can be
    /// inspected, then the requested checkpoint, then the network's latest),
    /// opens or creates the data directory, loads any requested seeds, starts
    /// the embedded indexer, binds the listener, and serves. Startup fails
    /// rather than reinterpreting a data directory that describes a different
    /// network or checkpoint than the one requested.
    ///
    /// `version` is reported as the server version of the gRPC service and in
    /// requests to the live network's GraphQL endpoint. Metrics for the
    /// RPC server, the subscription broker, and the embedded indexer are
    /// registered into `registry`; use one registry per fork, since a second
    /// registration of the same collectors fails.
    ///
    /// The listener is bound before anything durable is touched, so the most
    /// common environmental failure — the port is already taken — errors
    /// before a data directory, seed manifest, or metric registration exists,
    /// and a retry starts from a clean slate. The bound port accepts TCP
    /// connections from that moment: a client that connects while startup is
    /// still initializing queues in the accept backlog instead of being
    /// refused, so give clients request deadlines rather than treating a
    /// successful connect as readiness.
    ///
    /// No signal handlers are installed and tracing is never initialized —
    /// both belong to the embedding binary. The fork's background tasks live
    /// in the returned handle and are cleaned up when it is dropped, shut
    /// down, or converted into a [`Service`].
    pub async fn start(
        args: StartArgs,
        version: &'static str,
        registry: &Registry,
    ) -> Result<ForkNode> {
        let StartArgs {
            network,
            checkpoint,
            data_dir,
            addresses,
            object_ids,
            rpc_listen_address,
        } = args;
        let seed_input = SeedInput {
            addresses: addresses.into_iter().collect(),
            object_ids: object_ids.into_iter().collect(),
        };

        let listener = startup::bind(rpc_listen_address).await?;
        let parts =
            startup::initialize(network, checkpoint, version, data_dir, seed_input, registry)
                .await?;
        Self::from_parts(parts, listener, version, registry).await
    }

    /// Serve an already-initialized fork. The seam between building fork state
    /// and serving it: production goes through [`Self::start`], tests inject a
    /// hand-built [`ForkParts`] over synthetic genesis state.
    pub(crate) async fn from_parts(
        parts: ForkParts,
        listener: TcpListener,
        version: &'static str,
        registry: &Registry,
    ) -> Result<ForkNode> {
        let ForkParts {
            context,
            subscription_handle,
            indexer_service,
            data_dir,
            network_name,
            forked_at_checkpoint,
            starting_checkpoint,
            resumed,
        } = parts;

        let context = Arc::new(context);
        let (rpc_address, server) = startup::serve(
            Arc::clone(&context),
            subscription_handle,
            listener,
            version,
            registry,
        )
        .await?;

        Ok(ForkNode {
            admin: ForkAdmin::new(context),
            service: server.merge(indexer_service),
            rpc_address,
            network_name,
            data_dir,
            forked_at_checkpoint,
            starting_checkpoint,
            resumed,
        })
    }

    /// The address the gRPC server is bound to. When the configured listen
    /// address had port 0, this carries the ephemeral port that was selected.
    pub fn rpc_address(&self) -> SocketAddr {
        self.rpc_address
    }

    /// Name of the live network (`mainnet`, `testnet`, `devnet`, or the
    /// custom endpoint URL).
    pub fn network_name(&self) -> &str {
        &self.network_name
    }

    /// The resolved directory holding this fork's persistent state.
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// The checkpoint the fork was created from.
    pub fn forked_at_checkpoint(&self) -> CheckpointSequenceNumber {
        self.forked_at_checkpoint
    }

    /// The fork's local checkpoint tip when it started. Equal to
    /// [`Self::forked_at_checkpoint`] on a fresh fork, higher on a resumed one.
    pub fn starting_checkpoint(&self) -> CheckpointSequenceNumber {
        self.starting_checkpoint
    }

    /// Whether the fork resumed state persisted by an earlier run.
    pub fn resumed(&self) -> bool {
        self.resumed
    }

    /// Advance the fork's clock by `duration` and seal the resulting clock
    /// transaction into a checkpoint. Same contract as the forking gRPC
    /// service's `AdvanceClock`; both delegate to one implementation.
    ///
    /// The result is durable when this returns. If the embedded indexer fails
    /// to index the sealed checkpoint in time, the call still succeeds after
    /// logging an error; until the indexer catches up, derived reads (owned
    /// objects, balances) lag raw reads and subscribers are not notified.
    pub async fn advance_clock(&self, duration: Duration) -> ClockAdvanced {
        self.admin.advance_clock(duration).await
    }

    /// Seal all pending transactions into a new checkpoint. Same contract as
    /// the forking gRPC service's `AdvanceCheckpoint`; the indexing caveat on
    /// [`Self::advance_clock`] applies here too.
    pub async fn create_checkpoint(&self) -> CreatedCheckpoint {
        self.admin.create_checkpoint().await
    }

    /// The fork's current epoch, checkpoint tip, clock, and fork point. Same
    /// contract as the forking gRPC service's `GetStatus`.
    pub async fn status(&self) -> ForkStatus {
        self.admin.status().await
    }

    /// Resolves with the error when one of the fork's tasks — the server or
    /// an indexer pipeline — fails, or with `Ok(())` once every task has
    /// completed. A panicked task resumes unwinding here. A wedged indexer
    /// does not surface through this method: its framework retries failures
    /// indefinitely, so a stall shows up as error logs and 30-second waits on
    /// checkpoint-producing calls instead.
    pub async fn join(&mut self) -> Result<()> {
        self.service.join().await
    }

    /// Shut the fork down gracefully: stop accepting RPCs, drain in-flight
    /// requests, and wind down the embedded indexer.
    ///
    /// Transactions executed but not yet sealed into a checkpoint are lost,
    /// exactly as they are on process exit; call [`Self::create_checkpoint`]
    /// first to keep them. Everything sealed is already durable.
    pub async fn shutdown(self) -> Result<()> {
        self.service.shutdown().await.map_err(Into::into)
    }

    /// Surrender the fork's tasks as a [`Service`] for composition with other
    /// services (`host_service.merge(fork.into_service())`). The gRPC surface
    /// at [`Self::rpc_address`] remains, including the forking admin service;
    /// the in-process admin methods do not survive the conversion.
    pub fn into_service(self) -> Service {
        self.service
    }
}

impl ForkAdmin {
    pub(crate) fn new(context: Arc<Context>) -> Self {
        Self { context }
    }

    /// See [`ForkNode::advance_clock`].
    pub(crate) async fn advance_clock(&self, duration: Duration) -> ClockAdvanced {
        let ((tx_digest, timestamp_ms), checkpoint) = self
            .context
            .run_with_new_checkpoint(|sim| {
                let effects = sim.advance_clock(duration);
                let tx_digest = *effects.transaction_digest();
                let timestamp_ms = sim.store().get_clock().timestamp_ms;
                (tx_digest, timestamp_ms)
            })
            .await;

        info!(
            %tx_digest,
            duration_ms = duration.as_millis() as u64,
            timestamp_ms,
            checkpoint_sequence_number = checkpoint.sequence_number,
            "clock advanced"
        );

        ClockAdvanced {
            tx_digest,
            timestamp_ms,
            checkpoint,
        }
    }

    /// See [`ForkNode::create_checkpoint`].
    pub(crate) async fn create_checkpoint(&self) -> CreatedCheckpoint {
        let ((), checkpoint) = self.context.run_with_new_checkpoint(|_| ()).await;

        info!(
            checkpoint_sequence_number = checkpoint.sequence_number,
            timestamp_ms = checkpoint.timestamp_ms,
            "checkpoint created"
        );

        checkpoint
    }

    /// See [`ForkNode::status`].
    pub(crate) async fn status(&self) -> ForkStatus {
        let sim = self.context.simulacrum().read().await;
        ForkStatus {
            epoch: sim.epoch_start_state().epoch(),
            checkpoint_sequence_number: sim
                .store()
                .get_highest_checkpint()
                .map(|checkpoint| checkpoint.data().sequence_number)
                .unwrap_or(0),
            timestamp_ms: sim.store().get_clock().timestamp_ms,
            forked_at_checkpoint: sim.store().forked_at_checkpoint(),
        }
    }
}

// ============================================================================
// Read traits
// ============================================================================

/// Signed transaction envelope paired with its execution effects and the checkpoint it was
/// finalized in. The checkpoint is used by [`crate::store::ForkStore`] as a pre-fork guard, because
/// remote results whose `checkpoint > forked_at_checkpoint` must not leak into a fork that has
/// already diverged from the upstream chain.
#[derive(Clone, Debug)]
pub(crate) struct TransactionInfo {
    pub(crate) transaction: VerifiedTransaction,
    pub(crate) effects: TransactionEffects,
    pub(crate) checkpoint: CheckpointSequenceNumber,
}

/// `TransactionRead` trait is used to retrieve transaction data for a given digest.
pub(crate) trait TransactionRead {
    /// Given a transaction digest, return the signed transaction, its effects, and the checkpoint
    /// it was finalized in. Returns `None` if the transaction is not found.
    fn transaction_data_and_effects(
        &self,
        tx_digest: &str,
    ) -> Result<Option<TransactionInfo>, Error>;
}

/// Query for an object, specifying an `ObjectID` and the rule to retrieve it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ObjectKey {
    pub(crate) object_id: ObjectID,
    pub(crate) version_query: VersionQuery,
}

/// Version rule for an object query.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum VersionQuery {
    /// Request the highest version of the object at or below the given version.
    RootVersion(u64),

    /// Request the object as of the given checkpoint. Useful when the version is unknown.
    AtCheckpoint(u64),

    /// Request an exact version, but only if it existed by the given checkpoint.
    VersionAtCheckpoint { version: u64, checkpoint: u64 },
}

/// The `ObjectRead` trait is used to retrieve objects by their keys, with different query options.
pub(crate) trait ObjectRead {
    /// Retrieve objects by their keys, with different query options.
    ///
    /// If the object is not found, the element in the vector is `None`. Otherwise each tuple
    /// contains:
    /// - `Object`: The object data
    /// - `u64`: The actual version of the object
    fn get_objects(&self, keys: &[ObjectKey]) -> Result<Vec<Option<(Object, u64)>>, Error>;
}

/// Checkpoint read data.
pub(crate) trait CheckpointRead {
    /// Return the verified checkpoint summary together with its decoded contents. If `sequence` is
    /// `None`, return the latest checkpoint.
    fn get_checkpoint(
        &self,
        sequence: Option<CheckpointSequenceNumber>,
    ) -> Result<Option<(VerifiedCheckpoint, CheckpointContents)>, Error>;
}
