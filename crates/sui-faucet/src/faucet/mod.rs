// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::FaucetError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};
use uuid::Uuid;

mod simple_faucet;
mod write_ahead_log;
pub use self::simple_faucet::SimpleFaucet;
use clap::Parser;
use std::{net::Ipv4Addr, path::PathBuf};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FaucetReceipt {
    pub sent: Vec<CoinInfo>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CoinInfo {
    pub amount: u64,
    pub id: ObjectID,
    pub transfer_tx_digest: TransactionDigest,
}

#[async_trait]
pub trait Faucet {
    /// Send `Coin<SUI>` of the specified amount to the recipient
    async fn send(
        &self,
        id: Uuid,
        recipient: SuiAddress,
        amounts: &[u64],
    ) -> Result<FaucetReceipt, FaucetError>;
}

#[derive(Parser, Clone)]
#[clap(
    name = "Sui Faucet",
    about = "Faucet for requesting test tokens on Sui",
    rename_all = "kebab-case"
)]
pub struct FaucetConfig {
    #[clap(long, default_value_t = 5003)]
    pub port: u16,

    #[clap(long, default_value = "127.0.0.1")]
    pub host_ip: Ipv4Addr,

    #[clap(long, default_value_t = 200_000_000)]
    pub amount: u64,

    #[clap(long, default_value_t = 5)]
    pub num_coins: usize,

    #[clap(long, default_value_t = 10)]
    pub request_buffer_size: usize,

    #[clap(long, default_value_t = 10)]
    pub max_request_per_second: u64,

    #[clap(long, default_value_t = 60)]
    pub wallet_client_timeout_secs: u64,

    #[clap(long)]
    pub write_ahead_log: PathBuf,

    #[clap(long, default_value_t = 300)]
    pub wal_retry_interval: u64,
}

impl Default for FaucetConfig {
    fn default() -> Self {
        Self {
            port: 5003,
            host_ip: Ipv4Addr::new(127, 0, 0, 1),
            amount: 200_000_000,
            num_coins: 5,
            request_buffer_size: 10,
            max_request_per_second: 10,
            wallet_client_timeout_secs: 60,
            write_ahead_log: Default::default(),
            wal_retry_interval: 300,
        }
    }
}
