// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use args::Args;
use clap::Parser;
use plan::CutPlan;

mod args;
mod path;
mod plan;

fn main() {
    let args = Args::parse();
    println!("Cutting directories: {:#?}\n", args.directories);
    println!("Including packages: {:#?}\n", args.packages);

    let plan = CutPlan::discover(args);
    println!("Plan: {:#?}\n", plan);
}
