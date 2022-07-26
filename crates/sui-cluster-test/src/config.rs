// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use clap::*;

#[derive(Parser, Clone, ArgEnum)]
pub enum Env {
    DevNet,
    Staging,
    Continuous,
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
}
