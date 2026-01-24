// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::validator_commands::{
    SuiValidatorCommand, SuiValidatorCommandResponse, get_validator_summary,
};
use anyhow::Ok;
use fastcrypto::encoding::{Base64, Encoding};
use shared_crypto::intent::{Intent, IntentMessage};
use sui_types::crypto::SuiKeyPair;
use sui_types::transaction::TransactionData;
use sui_types::{base_types::SuiAddress, crypto::Signature, transaction::Transaction};
use test_cluster::TestClusterBuilder;

#[tokio::test]
async fn test_print_raw_rgp_txn() -> Result<(), anyhow::Error> {
    let test_cluster = TestClusterBuilder::new().build().await;
    let keypair: &SuiKeyPair = test_cluster
        .swarm
        .config()
        .validator_configs
        .first()
        .unwrap()
        .account_key_pair
        .keypair();
    let validator_address: SuiAddress = SuiAddress::from(&keypair.public());
    let mut context = test_cluster.wallet;
    let sui_client = context.get_client().await?;
    let (_, summary) = get_validator_summary(&sui_client, validator_address)
        .await?
        .unwrap();
    let operation_cap_id = summary.operation_cap_id;

    // Execute the command and get the serialized transaction data.
    let response = SuiValidatorCommand::DisplayGasPriceUpdateRawTxn {
        sender_address: validator_address,
        new_gas_price: 42,
        operation_cap_id,
        gas_budget: None,
    }
    .execute(&mut context)
    .await?;
    let SuiValidatorCommandResponse::DisplayGasPriceUpdateRawTxn {
        data,
        serialized_data,
    } = response
    else {
        panic!("Expected DisplayGasPriceUpdateRawTxn");
    };

    // Construct the signed transaction and execute it.
    let deserialized_data =
        bcs::from_bytes::<TransactionData>(&Base64::decode(&serialized_data).unwrap())?;
    let signature = Signature::new_secure(
        &IntentMessage::new(Intent::sui_transaction(), deserialized_data),
        keypair,
    );
    let txn = Transaction::from_data(data, vec![signature]);
    context.execute_transaction_must_succeed(txn).await;
    let (_, summary) = get_validator_summary(&sui_client, validator_address)
        .await?
        .unwrap();

    // Check that the gas price is updated correctly.
    assert_eq!(summary.next_epoch_gas_price, 42);
    Ok(())
}

#[tokio::test]
async fn test_serialize_unsigned_transaction() -> Result<(), anyhow::Error> {
    let test_cluster = TestClusterBuilder::new().build().await;
    let keypair: &SuiKeyPair = test_cluster
        .swarm
        .config()
        .validator_configs
        .first()
        .unwrap()
        .account_key_pair
        .keypair();
    let validator_address: SuiAddress = SuiAddress::from(&keypair.public());
    let mut context = test_cluster.wallet;
    let sui_client = context.get_client().await?;
    let (_, summary) = get_validator_summary(&sui_client, validator_address)
        .await?
        .unwrap();
    let operation_cap_id = summary.operation_cap_id;

    // Test UpdateGasPrice serialization
    let response = SuiValidatorCommand::UpdateGasPrice {
        operation_cap_id: Some(operation_cap_id),
        gas_price: 100,
        tx_args: crate::validator_commands::TxProcessingArgs {
            serialize_unsigned_transaction: true,
            gas_budget: None,
        },
    }
    .execute(&mut context)
    .await?;

    if let SuiValidatorCommandResponse::UpdateGasPrice {
        response: _,
        serialized_unsigned_transaction,
    } = response
    {
        assert!(serialized_unsigned_transaction.is_some());
        // Verify we can deserialize it back
        let serialized_data = serialized_unsigned_transaction.unwrap();
        let _deserialized_data =
            bcs::from_bytes::<TransactionData>(&Base64::decode(&serialized_data).unwrap())?;
    } else {
        panic!("Expected UpdateGasPrice response with serialized transaction");
    }

    // Test ReportValidator serialization
    // We need another validator address to report
    let other_validator_address: SuiAddress = SuiAddress::from(
        &test_cluster
            .swarm
            .config()
            .validator_configs
            .get(1)
            .unwrap()
            .account_key_pair
            .keypair()
            .public(),
    );

    let response = SuiValidatorCommand::ReportValidator {
        operation_cap_id: Some(operation_cap_id),
        reportee_address: other_validator_address,
        undo_report: None,
        tx_args: crate::validator_commands::TxProcessingArgs {
            serialize_unsigned_transaction: true,
            gas_budget: None,
        },
    }
    .execute(&mut context)
    .await?;

    if let SuiValidatorCommandResponse::ReportValidator {
        response: _,
        serialized_unsigned_transaction,
    } = response
    {
        assert!(serialized_unsigned_transaction.is_some());
        let serialized_data = serialized_unsigned_transaction.unwrap();
        let _deserialized_data =
            bcs::from_bytes::<TransactionData>(&Base64::decode(&serialized_data).unwrap())?;
    } else {
        panic!("Expected ReportValidator response with serialized transaction");
    }

    Ok(())
}
