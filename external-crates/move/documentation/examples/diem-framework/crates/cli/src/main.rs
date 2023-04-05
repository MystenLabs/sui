// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::Parser;
use move_cli::{Command, Move};
use move_core_types::{errmap::ErrorMapping, language_storage::CORE_CODE_ADDRESS};
use move_vm_test_utils::gas_schedule::zero_cost_schedule;

#[derive(Parser)]
pub struct DfCli {
    #[clap(flatten)]
    move_args: Move,

    #[clap(subcommand)]
    cmd: DfCommands,
}

#[derive(Parser)]
pub enum DfCommands {
    #[clap(flatten)]
    Command(Command),
    // extra commands available only in df-cli can be added below
}

fn main() -> Result<()> {
    // let error_descriptions: ErrorMapping =
    //     bcs::from_bytes(diem_framework_releases::current_error_descriptions())?;

    let natives = move_stdlib::natives::all_natives(
        CORE_CODE_ADDRESS,
        // We may want to switch to a different gas schedule in the future, but for now,
        // the all-zero one should be enough.
        move_stdlib::natives::GasParameters::zeros(),
    )
    .into_iter()
    .chain(move_stdlib::natives::nursery_natives(
        CORE_CODE_ADDRESS,
        // We may want to switch to a different gas schedule in the future, but for now,
        // the all-zero one should be enough.
        move_stdlib::natives::NurseryGasParameters::zeros(),
    ))
    .chain(diem_framework_natives::all_natives(CORE_CODE_ADDRESS))
    .collect::<Vec<_>>();

    let args = DfCli::parse();
    match args.cmd {
        DfCommands::Command(cmd) => move_cli::run_cli(
            natives,
            &zero_cost_schedule(),
            // TODO: implement this
            &ErrorMapping::default(),
            args.move_args,
            cmd,
        ),
    }
}
