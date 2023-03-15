// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::base_types::{ObjectDigest, ObjectID, SequenceNumber, SuiAddress};
use sui_types::messages::TransactionData;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;

use crate::operations::Operations;
use crate::types::ConstructionMetadata;

#[tokio::test]
async fn test_operation_data_parsing() -> Result<(), anyhow::Error> {
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
    let data = TransactionData::new_programmable_with_dummy_gas_price(sender, vec![gas], pt, 1000);

    let ops: Operations = data.clone().try_into()?;
    let metadata = ConstructionMetadata {
        sender,
        coins: vec![gas],
        objects: vec![],
        total_coin_value: 0,
        gas_price: 1,
        budget: 1000,
    };
    let parsed_data = ops.into_internal()?.try_into_data(metadata)?;
    assert_eq!(data, parsed_data);

    Ok(())
}
