// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Arguments for starting a fork.
//!
//! [`StartArgs`] describes what to fork and where to serve it. It derives `clap::Args` so a binary
//! can flatten it into its own command line, and `Default` so a library caller can construct it
//! directly; clap reads its defaults from the `Default` impl, so the two cannot drift.

use std::net::SocketAddr;
use std::path::PathBuf;

use sui_types::base_types::ObjectID;
use sui_types::base_types::SuiAddress;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

use crate::Network;

/// Default address the fork's RPC server binds when none is configured.
///
/// The port matches the fullnode RPC default (9000) on purpose: the fork stands in for a
/// fullnode, so clients that default to `localhost:9000` reach it unchanged. Pass `--rpc-addr`
/// when running next to a real local fullnode or `sui start`, which claim the same port.
pub const DEFAULT_RPC_ADDR: &str = "127.0.0.1:9000";

/// Everything needed to start a fork node.
///
/// The defaults fork mainnet at its latest checkpoint, store fork state under the default data
/// root, seed nothing, and serve on `127.0.0.1:9000`.
#[derive(clap::Args, Clone, Debug)]
pub struct StartArgs {
    /// Network to fork from: mainnet, testnet, devnet, or a custom GraphQL URL.
    #[arg(long, default_value_t = Self::default().network)]
    pub network: Network,

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

impl Default for StartArgs {
    fn default() -> Self {
        Self {
            network: Network::Mainnet,
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
