// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use sui_indexer_alt::args::Args;

fn main() {
    let args = Args::parse();

    println!("Hello, remote-store-url: {}!", args.remote_store_url);
}
