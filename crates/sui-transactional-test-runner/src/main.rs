// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;

fn main() -> anyhow::Result<()> {
    let ptb = ProgrammableTransactionBuilder::new();
    let ptb = ptb.finish();
    if ptb.commands.is_empty() {
        println!(
            "No commands in the programmable transaction. This is a small utility to generate transactional tests via a Rust ProgrammableTransactionBuilder. Add commands here before using."
        );
        return Ok(());
    }
    let mut output = String::new();
    sui_transactional_test_runner::programmable_transaction_test_parser::printer::commands(
        &BTreeMap::new(),
        &mut output,
        &ptb.commands,
    )
    .unwrap();
    println!("{output}");
    Ok(())
}
