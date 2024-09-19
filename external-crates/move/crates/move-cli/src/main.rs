// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use move_core_types::account_address::AccountAddress;
use move_vm_runtime::natives::move_stdlib::{stdlib_native_function_table, GasParameters};

fn main() -> Result<()> {
    let cost_table = &move_vm_test_utils::gas_schedule::INITIAL_COST_SCHEDULE;
    let addr = AccountAddress::from_hex_literal("0x1").unwrap();
    let natives =
        stdlib_native_function_table(addr, GasParameters::zeros(), /* silent */ false);

    move_cli::move_cli(natives, cost_table)
}
