// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use clap::*;

#[derive(Parser, Clone, ArgEnum)]
pub enum Env {
    Devnet,
    Staging,
    Ci,
    Testnet,
    CustomRemote,
    NewLocal,
}

#[derive(Parser)]
#[clap(name = "", rename_all = "kebab-case")]
pub struct ClusterTestOpt {
    #[clap(arg_enum)]
    pub env: Env,
    #[clap(long)]
    pub gateway_address: Option<String>,
    #[clap(long)]
    pub faucet_address: Option<String>,
    #[clap(long)]
    pub fullnode_address: Option<String>,
    #[clap(long)]
    pub websocket_address: Option<String>,
}

impl ClusterTestOpt {
    pub fn new_local() -> Self {
        Self {
            env: Env::NewLocal,
            gateway_address: None,
            faucet_address: None,
            fullnode_address: None,
            websocket_address: None,
        }
    }
}
