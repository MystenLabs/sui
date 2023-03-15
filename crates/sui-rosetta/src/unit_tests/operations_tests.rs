// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::encoding::{Encoding, Hex};

use shared_crypto::intent::IntentMessage;
use sui_types::base_types::{ObjectDigest, ObjectID, SequenceNumber, SuiAddress};
use sui_types::messages::TransactionData;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;

use crate::operations::Operations;
use crate::types::{ConstructionMetadata, OperationType};

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
#[tokio::test]
async fn test_shorter_bytearray_bug() {
    // Sometime CallArg::Pure(Vec<u8>) for u64 will serialise to 8 bytes array instead of 9 bytes (length + data), this is to test the work around until we fix it in Sui Json.
    let bytes = "0x00000000000200208c0e814842a1b1e2d9870983dafc238bfda4d38feb5ac6bb32371c21eaebc68e0008077600000000000002020001010001010200000100008043cfb5f8976fe5602d4ad2c12545c8a3021ad87e79376a26ebc627b22b39510178791fd3cb356712ef23ec2c97c07b0c7c21d5853d6735ae794fb9f646670e0304000000000000002097e15e12b4a173a1755c747b1ae7b4e43518324bd4dfbe3be120062ce25861688043cfb5f8976fe5602d4ad2c12545c8a3021ad87e79376a26ebc627b22b395101000000000000009b0000000000000000";
    let hex = Hex::decode(bytes).unwrap();
    let data: IntentMessage<TransactionData> = bcs::from_bytes(&hex).unwrap();

    let op = Operations::try_from(data.value).unwrap();
    assert_eq!(OperationType::PaySui, op.type_().unwrap());
}
