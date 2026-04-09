// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod addr_balance;
mod coin_reservation;
mod common;
mod gasless;

pub use addr_balance::addr_balance_transaction_data_strategy;
pub use common::TxFuzzContext;
pub use gasless::gasless_transaction_data_strategy;

/// Default number of fuzz iterations per test. Can be overridden via `FUZZ_ITERATIONS` env var.
pub fn fuzz_iterations() -> usize {
    std::env::var("FUZZ_ITERATIONS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1000)
}
