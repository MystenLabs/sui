// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::base_types::{ObjectDigest, ObjectID, SequenceNumber, SuiAddress};
use sui_types::messages::TransactionData;

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

    let data = TransactionData::new_pay_sui(
        sender,
        vec![gas],
        vec![SuiAddress::random_for_testing_only()],
        vec![10000],
        gas,
        1000,
    );

    let ops: Operations = data.clone().try_into()?;
    let metadata = ConstructionMetadata {
        sender_coins: vec![gas],
    };

    let parsed_data = ops.into_transaction_data(metadata)?;
    assert_eq!(data, parsed_data);

    Ok(())
}
