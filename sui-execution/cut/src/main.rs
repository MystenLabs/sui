// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use args::Args;
use clap::Parser;
use plan::CutPlan;

mod args;
mod path;
mod plan;

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let dry_run = args.dry_run;
    let plan = CutPlan::discover(args)?;

    if dry_run {
        println!("{plan}");
    } else {
        plan.execute()?;
    }

    Ok(())
}
