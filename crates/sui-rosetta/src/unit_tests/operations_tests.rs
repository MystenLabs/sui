// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use sui_types::base_types::{
    ObjectDigest, ObjectID, ObjectInfo, SequenceNumber, SuiAddress, TransactionDigest,
};
use sui_types::gas_coin::GasCoin;
use sui_types::messages::TransactionData;
use sui_types::object::Owner;

use crate::operations::Operation;
use crate::types::ConstructionMetadata;

#[tokio::test]
async fn test_operation_data_parsing() -> Result<(), anyhow::Error> {
    let gas = (
        ObjectID::random(),
        SequenceNumber::new(),
        ObjectDigest::random(),
    );

    let sender = SuiAddress::random_for_testing_only();

    let data = TransactionData::new_transfer_sui(
        SuiAddress::random_for_testing_only(),
        sender,
        Some(10000),
        gas,
        1000,
    );

    let ops = Operation::from_data(&data)?;

    let metadata = ConstructionMetadata {
        input_objects: BTreeMap::from([gas].map(|obj| {
            (
                obj.0,
                ObjectInfo {
                    object_id: obj.0,
                    version: obj.1,
                    digest: obj.2,
                    type_: GasCoin::type_().to_string(),
                    owner: Owner::AddressOwner(sender),
                    previous_transaction: TransactionDigest::random(),
                },
            )
        })),
    };

    let parsed_data = Operation::parse_transaction_data(ops, metadata)
        .await
        .unwrap();
    assert_eq!(data, parsed_data);

    Ok(())
}
