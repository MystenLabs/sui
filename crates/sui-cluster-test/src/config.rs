// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anyhow::{anyhow, bail};
use clap::{ArgEnum, Parser};

#[derive(Parser, Clone, ArgEnum, Debug, strum_macros::Display)]
pub enum Env {
    Devnet,
    Staging,
    Ci,
    Testnet,
    CustomRemote,
    NewLocal,
}

#[derive(Parser, Clone, Debug)]
#[clap(name = "", rename_all = "kebab-case")]
pub struct ClusterTestOpt {
    #[clap(arg_enum)]
    pub env: Env,
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
            faucet_address: None,
            fullnode_address: None,
            websocket_address: None,
        }
    }
}

impl TryFrom<&String> for ClusterTestOpt {
    type Error = anyhow::Error;
    fn try_from(env: &String) -> Result<Self, Self::Error> {
        match Env::from_str(env, true).map_err(|_| anyhow!("Failed to parse {env} as Env"))? {
            Env::CustomRemote | Env::NewLocal => {
                bail!("Can't parse from Env::CustomRemote | Env::NewLocal");
            }
            other_env => Ok(ClusterTestOpt {
                env: other_env,
                faucet_address: None,
                fullnode_address: None,
                websocket_address: None,
            }),
        }
    }
}
