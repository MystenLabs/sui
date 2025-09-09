// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use std::net::Ipv4Addr;

pub const DEFAULT_AMOUNT: u64 = 200_000_000_000;
pub const DEFAULT_NUM_COINS: usize = 5;

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

    /// Amount of SUI per each dispensed coin.
    #[clap(long, default_value_t = DEFAULT_AMOUNT)]
    pub amount: u64,

    #[clap(long, default_value_t = DEFAULT_NUM_COINS)]
    pub num_coins: usize,

    #[clap(long, default_value_t = 60)]
    pub wallet_client_timeout_secs: u64,
}

impl Default for FaucetConfig {
    fn default() -> Self {
        Self {
            port: 5003,
            host_ip: Ipv4Addr::new(127, 0, 0, 1),
            amount: DEFAULT_AMOUNT,
            num_coins: DEFAULT_NUM_COINS,
            wallet_client_timeout_secs: 60,
        }
    }
}
