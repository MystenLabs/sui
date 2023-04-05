// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use move_core_types::{account_address::AccountAddress, errmap::ErrorMapping};
use move_stdlib::natives::{all_natives, nursery_natives, GasParameters, NurseryGasParameters};

fn main() -> Result<()> {
    let error_descriptions: ErrorMapping = bcs::from_bytes(move_stdlib::error_descriptions())?;
    let cost_table = &move_vm_test_utils::gas_schedule::INITIAL_COST_SCHEDULE;
    let addr = AccountAddress::from_hex_literal("0x1").unwrap();
    let natives = all_natives(addr, GasParameters::zeros())
        .into_iter()
        .chain(nursery_natives(addr, NurseryGasParameters::zeros()))
        .collect();

    move_cli::move_cli(natives, cost_table, &error_descriptions)
}
