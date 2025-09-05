// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::annotated_value::MoveTypeLayout;
use sui_json_rpc_types::SuiCallArg;
use sui_types::base_types::{ObjectDigest, ObjectID, SequenceNumber, SuiAddress};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{CallArg, TransactionData, TEST_ONLY_GAS_UNIT_FOR_TRANSFER};

use crate::operations::Operations;
use crate::types::{ConstructionMetadata, OperationType};
use crate::SUI;

#[tokio::test]
async fn test_operation_data_parsing_pay_sui() -> Result<(), anyhow::Error> {
    let gas = (
        ObjectID::random(),
        SequenceNumber::new(),
        ObjectDigest::random(),
    );

    let sender = SuiAddress::random_for_testing_only();

    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder
            .pay_sui(vec![SuiAddress::random_for_testing_only()], vec![10000])
            .unwrap();
        builder.finish()
    };
    let gas_price = 10;
    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        pt,
        TEST_ONLY_GAS_UNIT_FOR_TRANSFER * gas_price,
        gas_price,
    );

    let ops: Operations = data.clone().try_into()?;
    ops.0
        .iter()
        .for_each(|op| assert_eq!(op.type_, OperationType::PaySui));
    let metadata = ConstructionMetadata {
        sender,
        coins: vec![gas],
        objects: vec![],
        total_coin_value: 0,
        gas_price,
        budget: TEST_ONLY_GAS_UNIT_FOR_TRANSFER * gas_price,
        currency: None,
    };
    let parsed_data = ops.into_internal()?.try_into_data(metadata)?;
    assert_eq!(data, parsed_data);

    Ok(())
}
#[tokio::test]
async fn test_operation_data_parsing_pay_coin() -> Result<(), anyhow::Error> {
    let gas = (
        ObjectID::random(),
        SequenceNumber::new(),
        ObjectDigest::random(),
    );

    let coin = (
        ObjectID::random(),
        SequenceNumber::new(),
        ObjectDigest::random(),
    );

    let sender = SuiAddress::random_for_testing_only();

    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder
            .pay(
                vec![coin],
                vec![SuiAddress::random_for_testing_only()],
                vec![10000],
            )
            .unwrap();
        // the following is important in order to be able to transfer the coin type info between the various flow steps
        builder.pure(serde_json::to_string(&SUI.clone())?)?;
        builder.finish()
    };
    let gas_price = 10;
    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        pt,
        TEST_ONLY_GAS_UNIT_FOR_TRANSFER * gas_price,
        gas_price,
    );

    let ops: Operations = data.clone().try_into()?;
    ops.0
        .iter()
        .for_each(|op| assert_eq!(op.type_, OperationType::PayCoin));
    let metadata = ConstructionMetadata {
        sender,
        coins: vec![gas],
        objects: vec![coin],
        total_coin_value: 0,
        gas_price,
        budget: TEST_ONLY_GAS_UNIT_FOR_TRANSFER * gas_price,
        currency: Some(SUI.clone()),
    };
    let parsed_data = ops.into_internal()?.try_into_data(metadata)?;
    assert_eq!(data, parsed_data);

    Ok(())
}
#[tokio::test]
async fn test_sui_json() {
    let arg1 = CallArg::Pure(bcs::to_bytes(&1000000u64).unwrap());
    let arg2 = CallArg::Pure(bcs::to_bytes(&30215u64).unwrap());
    let json1 = SuiCallArg::try_from(arg1, Some(&MoveTypeLayout::U64)).unwrap();
    let json2 = SuiCallArg::try_from(arg2, Some(&MoveTypeLayout::U64)).unwrap();
    println!("{:?}, {:?}", json1, json2);
}
