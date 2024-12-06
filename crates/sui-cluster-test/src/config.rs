// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use regex::Regex;
use std::path::PathBuf;

#[derive(Parser, Clone, ValueEnum, Debug)]
pub enum Env {
    Devnet,
    Staging,
    Ci,
    CiNomad,
    Testnet,
    CustomRemote,
    NewLocal,
}

#[derive(derive_more::Debug, Parser)]
#[clap(name = "", rename_all = "kebab-case")]
pub struct ClusterTestOpt {
    #[clap(value_enum)]
    pub env: Env,
    #[clap(long)]
    pub faucet_address: Option<String>,
    #[clap(long)]
    pub fullnode_address: Option<String>,
    #[clap(long)]
    pub epoch_duration_ms: Option<u64>,
    /// URL for the indexer RPC server
    #[clap(long)]
    pub indexer_address: Option<String>,
    /// URL for the Indexer Postgres DB
    #[clap(long)]
    #[debug("{}", match pg_address {
        None => "None".into(),
        Some(val) => Regex::new(r":.*@")
            .unwrap()
            .replace_all(val.as_str(), ":*****@"),
    })]
    pub pg_address: Option<String>,
    #[clap(long)]
    pub config_dir: Option<PathBuf>,
    /// URL for the indexer RPC server
    #[clap(long)]
    pub graphql_address: Option<String>,
    /// Indicate that an indexer and graphql service should be started
    ///
    /// Only used with a local cluster
    #[clap(long)]
    pub with_indexer_and_graphql: bool,
}

impl ClusterTestOpt {
    pub fn new_local() -> Self {
        Self {
            env: Env::NewLocal,
            faucet_address: None,
            fullnode_address: None,
            epoch_duration_ms: None,
            indexer_address: None,
            pg_address: None,
            config_dir: None,
            graphql_address: None,
            with_indexer_and_graphql: false,
        }
    }
}
