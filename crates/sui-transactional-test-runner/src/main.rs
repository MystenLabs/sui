use std::collections::BTreeMap;

use sui_types::{
    base_types::SuiAddress, programmable_transaction_builder::ProgrammableTransactionBuilder,
};

fn main() -> anyhow::Result<()> {
    let mut ptb = ProgrammableTransactionBuilder::new();
    let ptb = ptb.finish();
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
