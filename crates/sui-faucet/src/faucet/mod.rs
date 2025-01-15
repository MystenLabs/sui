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
use std::{net::Ipv4Addr, path::PathBuf, sync::Arc};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FaucetReceipt {
    pub sent: Vec<CoinInfo>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BatchFaucetReceipt {
    pub task: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CoinInfo {
    pub amount: u64,
    pub id: ObjectID,
    pub transfer_tx_digest: TransactionDigest,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BatchSendStatus {
    pub status: BatchSendStatusType,
    pub transferred_gas_objects: Option<FaucetReceipt>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum BatchSendStatusType {
    INPROGRESS,
    SUCCEEDED,
    DISCARDED,
}

pub struct AppState<F = Arc<SimpleFaucet>> {
    pub faucet: F,
    pub config: FaucetConfig,
}

impl<F> AppState<F> {
    pub fn new(faucet: F, config: FaucetConfig) -> Self {
        Self { faucet, config }
    }
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

    /// Send `Coin<SUI>` of the specified amount to the recipient in a batch request
    async fn batch_send(
        &self,
        id: Uuid,
        recipient: SuiAddress,
        amounts: &[u64],
    ) -> Result<BatchFaucetReceipt, FaucetError>;

    /// Get the status of a batch_send request
    async fn get_batch_send_status(&self, task_id: Uuid) -> Result<BatchSendStatus, FaucetError>;
}

pub const DEFAULT_AMOUNT: u64 = 1_000_000_000;
pub const DEFAULT_NUM_OF_COINS: usize = 1;

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

    #[clap(long, default_value_t = DEFAULT_AMOUNT)]
    pub amount: u64,

    #[clap(long, default_value_t = DEFAULT_NUM_OF_COINS)]
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

    #[clap(long, default_value_t = 10000)]
    pub max_request_queue_length: u64,

    #[clap(long, default_value_t = 500)]
    pub batch_request_size: u64,

    #[clap(long, default_value_t = 300)]
    pub ttl_expiration: u64,

    #[clap(long, action = clap::ArgAction::Set, default_value_t = false)]
    pub batch_enabled: bool,

    /// Testnet faucet requires authentication via the Web UI at <https://faucet.sui.io>
    /// This flag is used to indicate that authentication mode is enabled.
    #[clap(long)]
    pub authenticated: bool,

    /// Maximum number of requests per IP address. This is used for the authenticated mode.
    #[clap(long, default_value_t = 3)]
    pub max_requests_per_ip: u64,

    /// This is the amount of time to wait before adding one more quota to the rate limiter. Basically,
    /// it ensures that we're not allowing too many requests all at once. This is very specific to
    /// governor and tower-governor crates. This is used primarily for authenticated mode. A small
    /// value will allow more requests to be processed in a short period of time.
    #[clap(long, default_value_t = 10)]
    pub replenish_quota_interval_ms: u64,

    /// The amount of seconds to wait before resetting the request count for the IP addresses recorded
    /// by the rate limit layer. Default is 12 hours. This is used for authenticated mode.
    #[clap(long, default_value_t = 3600*12)]
    pub reset_time_interval_secs: u64,

    /// Interval time to run the task to clear the banned IP addresses by the rate limiter. This is
    /// used for authenticated mode.
    #[clap(long, default_value_t = 60)]
    pub rate_limiter_cleanup_interval_secs: u64,
}

impl Default for FaucetConfig {
    fn default() -> Self {
        Self {
            port: 5003,
            host_ip: Ipv4Addr::new(127, 0, 0, 1),
            amount: 1_000_000_000,
            num_coins: 1,
            request_buffer_size: 10,
            max_request_per_second: 10,
            wallet_client_timeout_secs: 60,
            write_ahead_log: Default::default(),
            wal_retry_interval: 300,
            max_request_queue_length: 10000,
            batch_request_size: 500,
            ttl_expiration: 300,
            batch_enabled: false,
            authenticated: false,
            max_requests_per_ip: 3,
            replenish_quota_interval_ms: 10,
            reset_time_interval_secs: 3600 * 12,
            rate_limiter_cleanup_interval_secs: 60,
        }
    }
}
