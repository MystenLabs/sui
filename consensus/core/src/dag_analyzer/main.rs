// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use tokio::time::Instant;

#[tokio::main]
pub async fn main() {
    let start_time = Instant::now();
    consensus_core::dag_analyzer::reader::read().await;
    let elapsed = start_time.elapsed();
    println!("Elapsed time: {:?}", elapsed);
}
