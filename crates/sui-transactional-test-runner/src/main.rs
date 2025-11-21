use std::collections::BTreeMap;

use sui_types::{
    base_types::SuiAddress, programmable_transaction_builder::ProgrammableTransactionBuilder,
};

fn main() -> anyhow::Result<()> {
    let recipients: [u8; 100] = [
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
        25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47,
        48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70,
        71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93,
        94, 95, 96, 97, 98, 99,
    ];
    let recipients: Vec<SuiAddress> = recipients
        .iter()
        .map(|b| {
            let mut bytes = [0u8; 32];
            bytes[0] = *b;
            SuiAddress::from_bytes(bytes).unwrap()
        })
        .collect();
    let mut pow = 1u8;
    let mut amounts: Vec<u64> = vec![];
    let mut cur = 0;
    while amounts.len() < recipients.len() {
        let mut n_cur = 0;
        while n_cur < pow && amounts.len() < recipients.len() {
            amounts.push(cur);
            n_cur += 1;
        }
        cur += 1;
        pow = pow << 1;
    }
    let mut ptb = ProgrammableTransactionBuilder::new();
    ptb.pay_sui(recipients, amounts)?;
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
