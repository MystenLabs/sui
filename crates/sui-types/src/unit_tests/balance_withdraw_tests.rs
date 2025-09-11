// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use crate::{
    accumulator_root::AccumulatorValue,
    base_types::{random_object_ref, SuiAddress},
    gas_coin::GAS,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{FundsWithdrawalArg, TransactionData, TransactionDataAPI, WithdrawalTypeArg},
    type_input::TypeInput,
};

#[test]
fn test_withdraw_max_amount() {
    let arg = FundsWithdrawalArg::balance_from_sender(100, TypeInput::from(GAS::type_tag()));
    let mut ptb = ProgrammableTransactionBuilder::new();
    ptb.funds_withdrawal(arg.clone()).unwrap();
    let sender = SuiAddress::random_for_testing_only();
    let tx =
        TransactionData::new_programmable(sender, vec![random_object_ref()], ptb.finish(), 1, 1);
    assert!(tx.has_funds_withdrawals());
    let withdraws = tx.process_funds_withdrawals().unwrap();
    let account_id = AccumulatorValue::get_field_id(
        sender,
        &WithdrawalTypeArg::Balance(GAS::type_tag().into())
            .to_type_tag()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(withdraws, BTreeMap::from([(account_id, 100)]));
}

#[test]
fn test_multiple_withdraws_same_account() {
    let arg1 = FundsWithdrawalArg::balance_from_sender(100, TypeInput::from(GAS::type_tag()));
    let arg2 = FundsWithdrawalArg::balance_from_sender(200, TypeInput::from(GAS::type_tag()));
    let mut ptb = ProgrammableTransactionBuilder::new();
    ptb.funds_withdrawal(arg1.clone()).unwrap();
    ptb.funds_withdrawal(arg2.clone()).unwrap();
    let sender = SuiAddress::random_for_testing_only();
    let tx =
        TransactionData::new_programmable(sender, vec![random_object_ref()], ptb.finish(), 1, 1);
    assert!(tx.has_funds_withdrawals());
    let withdraws = tx.process_funds_withdrawals().unwrap();
    let account_id = AccumulatorValue::get_field_id(
        sender,
        &WithdrawalTypeArg::Balance(GAS::type_tag().into())
            .to_type_tag()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(withdraws, BTreeMap::from([(account_id, 300)]));
}

#[test]
fn test_multiple_withdraws_different_accounts() {
    let arg1 = FundsWithdrawalArg::balance_from_sender(100, TypeInput::from(GAS::type_tag()));
    let arg2 = FundsWithdrawalArg::balance_from_sender(200, TypeInput::Bool);
    let mut ptb = ProgrammableTransactionBuilder::new();
    ptb.funds_withdrawal(arg1.clone()).unwrap();
    ptb.funds_withdrawal(arg2.clone()).unwrap();
    let sender = SuiAddress::random_for_testing_only();
    let tx =
        TransactionData::new_programmable(sender, vec![random_object_ref()], ptb.finish(), 1, 1);
    assert!(tx.has_funds_withdrawals());
    let withdraws = tx.process_funds_withdrawals().unwrap();
    let account_id1 = AccumulatorValue::get_field_id(
        sender,
        &WithdrawalTypeArg::Balance(GAS::type_tag().into())
            .to_type_tag()
            .unwrap(),
    )
    .unwrap();
    let account_id2 = AccumulatorValue::get_field_id(
        sender,
        &WithdrawalTypeArg::Balance(TypeInput::Bool)
            .to_type_tag()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        withdraws,
        BTreeMap::from([(account_id1, 100), (account_id2, 200),])
    );
}

#[test]
fn test_withdraw_zero_amount() {
    let arg = FundsWithdrawalArg::balance_from_sender(0, TypeInput::from(GAS::type_tag()));
    let mut ptb = ProgrammableTransactionBuilder::new();
    ptb.funds_withdrawal(arg.clone()).unwrap();
    let sender = SuiAddress::random_for_testing_only();
    let tx =
        TransactionData::new_programmable(sender, vec![random_object_ref()], ptb.finish(), 1, 1);
    assert!(tx.process_funds_withdrawals().is_err());
}

#[test]
fn test_withdraw_too_many_withdraws() {
    let mut ptb = ProgrammableTransactionBuilder::new();
    for _ in 0..11 {
        ptb.funds_withdrawal(FundsWithdrawalArg::balance_from_sender(
            100,
            TypeInput::from(GAS::type_tag()),
        ))
        .unwrap();
    }
    let sender = SuiAddress::random_for_testing_only();
    let tx =
        TransactionData::new_programmable(sender, vec![random_object_ref()], ptb.finish(), 1, 1);
    assert!(tx.process_funds_withdrawals().is_err());
}
