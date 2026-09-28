// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Arguments for starting a forked network.
//!
//! [`StartArgs`] serves both programmatic callers and command-line consumers because it derives
//! [`clap::Args`] and provides the same defaults as the `sui-fork start` command.

use std::net::SocketAddr;
use std::path::PathBuf;

use sui_types::base_types::ObjectID;
use sui_types::base_types::SuiAddress;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

use crate::Node;

/// Default address for the fork's RPC server.
pub const DEFAULT_RPC_ADDR: &str = "127.0.0.1:9000";

/// Configuration for starting a forked network.
#[derive(clap::Args, Clone, Debug)]
pub struct StartArgs {
    /// Network to fork from, which can be mainnet, testnet, devnet, or a custom GraphQL URL.
    #[arg(long, default_value = "mainnet")]
    pub network: Node,

    /// Checkpoint sequence number to fork at, or the latest checkpoint when omitted.
    #[arg(long)]
    pub checkpoint: Option<CheckpointSequenceNumber>,

    /// Directory for persistent fork data, or the platform-specific default when omitted.
    #[arg(long)]
    pub data_dir: Option<PathBuf>,

    /// Addresses whose owned objects are recorded in the seed manifest.
    #[arg(long = "address")]
    pub addresses: Vec<SuiAddress>,

    /// Object IDs to seed when the corresponding objects are address-owned.
    #[arg(long = "object")]
    pub object_ids: Vec<ObjectID>,

    /// Address for the fork's RPC server.
    #[arg(long, default_value = DEFAULT_RPC_ADDR)]
    pub rpc_addr: SocketAddr,
}

impl Default for StartArgs {
    fn default() -> Self {
        Self {
            network: Node::Mainnet,
            checkpoint: None,
            data_dir: None,
            addresses: Vec::new(),
            object_ids: Vec::new(),
            rpc_addr: DEFAULT_RPC_ADDR
                .parse()
                .expect("default RPC address should be valid"),
        }
    }
}
