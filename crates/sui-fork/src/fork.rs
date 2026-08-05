// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Programmatic entry point for running a fork node inside another program.
//!
//! [`ForkArgs`] describes what to fork and where to serve it; it derives
//! `clap::Args` so a binary can flatten it into its own command line, and
//! `Default` so a library caller can construct it directly — clap reads its
//! defaults from the `Default` impl, so the two cannot drift.
//! [`ForkNode::start`] performs the whole startup sequence and returns a
//! [`ForkNode`]: a running fork whose accessors report what was started, whose
//! admin methods drive the fork's clock and checkpoints in-process, and whose
//! lifecycle methods decide when it stops.

use std::net::SocketAddr;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use prometheus::Registry;

use sui_futures::service::Service;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SuiAddress;
use sui_types::digests::TransactionDigest;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

use crate::Node;
use crate::context::Context;
use crate::seed::SeedInput;
use crate::startup;
use crate::startup::ForkParts;

/// Default address the fork's RPC server binds when none is configured.
pub const DEFAULT_RPC_ADDR: &str = "127.0.0.1:9000";

/// Everything needed to start a fork node.
///
/// The defaults fork mainnet at its latest checkpoint, store fork state under the default data
/// root, seed nothing, and serve on `127.0.0.1:9000`.
#[derive(clap::Args, Clone, Debug)]
pub struct ForkArgs {
    /// Network to fork from: mainnet, testnet, devnet, or a custom GraphQL URL.
    #[arg(long, default_value_t = Self::default().network)]
    pub network: Node,

    /// Checkpoint sequence number to fork at. When omitted, the fork found in the data directory
    /// is resumed if there is one to inspect; otherwise the network's latest checkpoint is used.
    #[arg(long)]
    pub checkpoint: Option<CheckpointSequenceNumber>,

    /// Directory where fork state is persisted. When omitted, a per-fork directory keyed by
    /// network and checkpoint is created under `$SUI_FORK_DATA`, `$XDG_DATA_HOME`, or `$HOME`
    /// (`%APPDATA%` on Windows).
    #[arg(long)]
    pub data_dir: Option<PathBuf>,

    /// Address whose owned objects should be recorded in the seed manifest
    ///
    /// This can be specified multiple times to seed multiple addresses. Seeding addresses requires
    /// forking at a recent checkpoint (less than an hour old).
    #[arg(long = "address")]
    pub addresses: Vec<SuiAddress>,

    /// Object ID to fetch and seed if it is owned by an address
    ///
    /// This can be specified multiple times to seed multiple objects
    #[arg(long = "object")]
    pub object_ids: Vec<ObjectID>,

    /// Address the fork's gRPC server binds. Port 0 selects an ephemeral port, and the bound
    /// address is reported by `ForkNode::rpc_address`.
    #[arg(long = "rpc-addr", default_value_t = Self::default().rpc_listen_address)]
    pub rpc_listen_address: SocketAddr,
}

impl Default for ForkArgs {
    fn default() -> Self {
        Self {
            network: Node::Mainnet,
            checkpoint: None,
            data_dir: None,
            addresses: Vec::new(),
            object_ids: Vec::new(),
            rpc_listen_address: DEFAULT_RPC_ADDR
                .parse()
                .expect("default RPC address is valid"),
        }
    }
}

/// A running fork node: gRPC server bound and serving, embedded indexer
/// running.
///
/// Dropping the handle aborts the fork's tasks immediately. Deliberate exits
/// go through [`Self::shutdown`], or through the [`Service`] returned by
/// [`Self::into_service`] for callers that compose the fork with other
/// services and own their own signal handling.
pub struct ForkNode {
    context: Arc<Context>,
    service: Service,
    rpc_address: SocketAddr,
    network_name: String,
    data_dir: PathBuf,
    forked_at_checkpoint: CheckpointSequenceNumber,
    starting_checkpoint: CheckpointSequenceNumber,
    resumed: bool,
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
    /// requests to the forked-from network's GraphQL endpoint. Metrics for the
    /// RPC server, the subscription broker, and the embedded indexer are
    /// registered into `registry`; use one registry per fork, since a second
    /// registration of the same collectors fails.
    ///
    /// No signal handlers are installed and tracing is never initialized —
    /// both belong to the embedding binary. The fork's background tasks live
    /// in the returned handle and are cleaned up when it is dropped, shut
    /// down, or converted into a [`Service`].
    pub async fn start(
        args: ForkArgs,
        version: &'static str,
        registry: &Registry,
    ) -> Result<ForkNode> {
        let ForkArgs {
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

        let (forked_at_checkpoint, resumed) =
            startup::resolve_fork_point(&network, checkpoint, data_dir.as_deref(), version).await?;
        let parts = startup::initialize(
            network,
            forked_at_checkpoint,
            version,
            data_dir,
            seed_input,
            registry,
        )
        .await?;
        Self::from_parts(parts, resumed, rpc_listen_address, version, registry).await
    }

    /// Serve an already-initialized fork. The seam between building fork state
    /// and serving it: production goes through [`Self::start`], tests inject a
    /// hand-built [`ForkParts`] over synthetic genesis state.
    pub(crate) async fn from_parts(
        parts: ForkParts,
        resumed: bool,
        listen_address: SocketAddr,
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
        } = parts;

        let context = Arc::new(context);
        let (rpc_address, server) = startup::serve(
            context.clone(),
            subscription_handle,
            listen_address,
            version,
            registry,
        )
        .await?;

        Ok(ForkNode {
            context,
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

    /// Name of the forked-from network (`mainnet`, `testnet`, `devnet`, or the
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
    pub async fn advance_clock(&self, duration: Duration) -> Result<ClockAdvanced> {
        Ok(self.context.advance_clock(duration).await)
    }

    /// Seal all pending transactions into a new checkpoint. Same contract as
    /// the forking gRPC service's `AdvanceCheckpoint`.
    pub async fn create_checkpoint(&self) -> Result<CreatedCheckpoint> {
        Ok(self.context.create_checkpoint().await)
    }

    /// The fork's current epoch, checkpoint tip, clock, and fork point. Same
    /// contract as the forking gRPC service's `GetStatus`.
    pub async fn status(&self) -> Result<ForkStatus> {
        Ok(self.context.status().await)
    }

    /// Resolves when any of the fork's tasks stops: the task error if the
    /// server or an indexer pipeline failed, `Ok(())` if all tasks completed.
    /// Either way the fork is no longer serving; this is the liveness watchdog.
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
    /// services (`host_service.merge(fork.into_service())`). The in-process
    /// admin methods go with the handle; the gRPC surface at
    /// [`Self::rpc_address`] remains.
    pub fn into_service(self) -> Service {
        self.service
    }
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
