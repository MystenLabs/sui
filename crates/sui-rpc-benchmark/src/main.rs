// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;

use sui_rpc_benchmark::run_benchmarks; 

#[tokio::main]
async fn main() -> Result<()> {
    run_benchmarks()
}
