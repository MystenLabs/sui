// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::*;
use crate::bcs_value_converter::BcsConversionError;
use crate::committee::EpochId;
use crate::digests::*;
use crate::effects::*;
use crate::execution_status::ExecutionStatus;
use crate::gas::GasCostSummary;
use crate::gas::GasData;
use crate::messages_checkpoint::*;
use crate::programmable_transaction_builder::ProgrammableTransactionBuilder;
use crate::transaction::*;
use std::convert::TryFrom;

/// Test round-trip conversion: Rust type -> BCS bytes -> Value -> Rust type
fn test_round_trip<T>(original: T)
where
    T: serde::Serialize
        + serde::de::DeserializeOwned
        + TryFrom<sui_bcs::Value>
        + PartialEq
        + std::fmt::Debug,
    <T as TryFrom<sui_bcs::Value>>::Error: std::fmt::Debug,
{
    // Serialize to BCS bytes
    let encoded = bcs::to_bytes(&original).expect("Failed to serialize to BCS");

    // Parse with sui-bcs to get Value
    let yaml_content = include_str!("../../../sui-core/tests/staged/sui.yaml");
    let parser = sui_bcs::Parser::from_yaml(yaml_content).expect("Failed to parse YAML");

    // Get the type name for the type
    let type_name = std::any::type_name::<T>();
    let type_name = type_name.split("::").last().unwrap_or(type_name);

    let value = parser
        .parse(&encoded, type_name)
        .expect("Failed to parse BCS data");

    // Convert Value back to Rust type
    let converted: T = T::try_from(value).expect("Failed to convert from Value");

    // Check equality
    assert_eq!(original, converted, "Round-trip conversion failed");
}

#[test]
fn test_gas_cost_summary_round_trip() {
    let gas_cost = GasCostSummary {
        computation_cost: 1000,
        storage_cost: 2000,
        storage_rebate: 500,
        non_refundable_storage_fee: 100,
    };

    test_round_trip(gas_cost);
}

#[test]
fn test_gas_cost_summary_zero_values() {
    let gas_cost = GasCostSummary {
        computation_cost: 0,
        storage_cost: 0,
        storage_rebate: 0,
        non_refundable_storage_fee: 0,
    };

    test_round_trip(gas_cost);
}

#[test]
fn test_gas_cost_summary_max_values() {
    let gas_cost = GasCostSummary {
        computation_cost: u64::MAX,
        storage_cost: u64::MAX,
        storage_rebate: u64::MAX,
        non_refundable_storage_fee: u64::MAX,
    };

    test_round_trip(gas_cost);
}

#[test]
fn test_execution_status_success_round_trip() {
    let status = ExecutionStatus::Success;

    // Serialize to BCS bytes
    let encoded = bcs::to_bytes(&status).expect("Failed to serialize to BCS");

    // Parse with sui-bcs to get Value
    let yaml_content = include_str!("../../../sui-core/tests/staged/sui.yaml");
    let parser = sui_bcs::Parser::from_yaml(yaml_content).expect("Failed to parse YAML");

    let value = parser
        .parse(&encoded, "ExecutionStatus")
        .expect("Failed to parse BCS data");

    // Convert Value back to Rust type
    let converted = ExecutionStatus::try_from(value).expect("Failed to convert from Value");

    // Check equality
    assert_eq!(status, converted, "Round-trip conversion failed");
}

#[test]
fn test_gas_cost_summary_with_extra_fields() {
    use sui_bcs::Value;

    // Simulate a GasCostSummary from a future version with extra fields
    let mut fields = vec![
        ("computationCost".to_string(), Value::U64(1000)),
        ("storageCost".to_string(), Value::U64(2000)),
        ("storageRebate".to_string(), Value::U64(500)),
        ("nonRefundableStorageFee".to_string(), Value::U64(100)),
        // Extra field from future version - should be ignored
        ("futureField".to_string(), Value::U64(999)),
    ];

    let value = Value::Struct(fields);

    // Should successfully convert, ignoring the extra field
    let gas_cost = GasCostSummary::try_from(value).expect("Should handle extra fields");

    assert_eq!(gas_cost.computation_cost, 1000);
    assert_eq!(gas_cost.storage_cost, 2000);
    assert_eq!(gas_cost.storage_rebate, 500);
    assert_eq!(gas_cost.non_refundable_storage_fee, 100);
}

#[test]
fn test_gas_cost_summary_missing_field() {
    use sui_bcs::Value;

    // Missing required field
    let fields = vec![
        ("computationCost".to_string(), Value::U64(1000)),
        ("storageCost".to_string(), Value::U64(2000)),
        // Missing storageRebate
        ("nonRefundableStorageFee".to_string(), Value::U64(100)),
    ];

    let value = Value::Struct(fields);

    // Should fail with missing field error
    let result = GasCostSummary::try_from(value);
    assert!(result.is_err());

    if let Err(BcsConversionError::MissingField(field)) = result {
        assert_eq!(field, "storageRebate");
    } else {
        panic!("Expected MissingField error");
    }
}

#[test]
fn test_gas_cost_summary_wrong_type() {
    use sui_bcs::Value;

    // Wrong type for a field
    let fields = vec![
        ("computationCost".to_string(), Value::U32(1000)), // Wrong: U32 instead of U64
        ("storageCost".to_string(), Value::U64(2000)),
        ("storageRebate".to_string(), Value::U64(500)),
        ("nonRefundableStorageFee".to_string(), Value::U64(100)),
    ];

    let value = Value::Struct(fields);

    // Should fail with type mismatch error
    let result = GasCostSummary::try_from(value);
    assert!(result.is_err());

    if let Err(BcsConversionError::TypeMismatch { field, .. }) = result {
        assert_eq!(field, "computationCost");
    } else {
        panic!("Expected TypeMismatch error");
    }
}

#[test]
fn test_execution_status_unknown_variant() {
    use sui_bcs::Value;

    // Simulate an ExecutionStatus with an unknown variant from a future version
    let value = Value::Enum("FutureVariant".to_string(), Box::new(Value::Unit));

    // Should fail with unknown variant error
    let result = ExecutionStatus::try_from(value);
    assert!(result.is_err());

    if let Err(BcsConversionError::UnknownVariant { variant, .. }) = result {
        assert_eq!(variant, "FutureVariant");
    } else {
        panic!("Expected UnknownVariant error");
    }
}

#[test]
fn test_transaction_data_round_trip() {
    // Create a test transaction
    let mut ptb = ProgrammableTransactionBuilder::new();
    let recipient = SuiAddress::random_for_testing_only();
    let obj_arg = ptb
        .obj(ObjectArg::ImmOrOwnedObject((
            ObjectID::random(),
            Default::default(),
            ObjectDigest::random(),
        )))
        .unwrap();
    ptb.transfer_arg(recipient, obj_arg);

    let gas_data = GasData {
        payment: vec![(
            ObjectID::random(),
            Default::default(),
            ObjectDigest::random(),
        )],
        owner: SuiAddress::random_for_testing_only(),
        price: 1000,
        budget: 10000,
    };

    let tx_data_v1 = TransactionDataV1 {
        kind: TransactionKind::ProgrammableTransaction(ptb.finish()),
        sender: SuiAddress::random_for_testing_only(),
        gas_data,
        expiration: TransactionExpiration::None,
    };

    let tx_data = TransactionData::V1(tx_data_v1);

    // Serialize to BCS bytes
    let encoded = bcs::to_bytes(&tx_data).expect("Failed to serialize to BCS");

    // Parse with sui-bcs to get Value
    let yaml_content = include_str!("../../../sui-core/tests/staged/sui.yaml");
    let parser = sui_bcs::Parser::from_yaml(yaml_content).expect("Failed to parse YAML");

    let value = parser
        .parse(&encoded, "TransactionData")
        .expect("Failed to parse BCS data");

    // Convert Value back to Rust type
    let converted = TransactionData::try_from(value).expect("Failed to convert from Value");

    // Check equality
    assert_eq!(tx_data, converted, "Round-trip conversion failed");
}

#[test]
fn test_transaction_effects_round_trip() {
    use crate::effects::TestEffectsBuilder;
    use crate::object::Owner;

    // Create test TransactionEffectsV2 with non-trivial data
    let effects = TestEffectsBuilder::new(TransactionDigest::random())
        .with_status(ExecutionStatus::Success)
        .with_executed_epoch(5)
        .with_gas_used(GasCostSummary {
            computation_cost: 1000,
            storage_cost: 500,
            storage_rebate: 50,
            non_refundable_storage_fee: 10,
        })
        .with_created(vec![
            (
                (
                    ObjectID::random(),
                    SequenceNumber::from_u64(1),
                    ObjectDigest::random(),
                ),
                Owner::AddressOwner(SuiAddress::random_for_testing_only()),
            ),
            (
                (
                    ObjectID::random(),
                    SequenceNumber::from_u64(2),
                    ObjectDigest::random(),
                ),
                Owner::Shared {
                    initial_shared_version: SequenceNumber::from_u64(1),
                },
            ),
        ])
        .with_mutated(vec![(
            (
                ObjectID::random(),
                SequenceNumber::from_u64(3),
                ObjectDigest::random(),
            ),
            Owner::AddressOwner(SuiAddress::random_for_testing_only()),
        )])
        .with_deleted(vec![(
            ObjectID::random(),
            SequenceNumber::from_u64(4),
            ObjectDigest::random(),
        )])
        .with_shared_objects(vec![(
            ObjectID::random(),
            SequenceNumber::from_u64(5),
            ObjectDigest::random(),
        )])
        .with_dependencies(vec![
            TransactionDigest::random(),
            TransactionDigest::random(),
        ])
        .build();

    // Serialize to BCS bytes
    let encoded = bcs::to_bytes(&effects).expect("Failed to serialize to BCS");

    // Parse with sui-bcs to get Value
    let yaml_content = include_str!("../../../sui-core/tests/staged/sui.yaml");
    let parser = sui_bcs::Parser::from_yaml(yaml_content).expect("Failed to parse YAML");

    let value = parser
        .parse(&encoded, "TransactionEffects")
        .expect("Failed to parse BCS data");

    // Convert Value back to Rust type
    let converted = TransactionEffects::try_from(value).expect("Failed to convert from Value");

    // Check equality
    assert_eq!(effects, converted, "Round-trip conversion failed");
}

#[test]
fn test_checkpoint_summary_round_trip() {
    // Create test CheckpointSummary
    let checkpoint = CheckpointSummary {
        epoch: 1,
        sequence_number: 100,
        network_total_transactions: 1000,
        content_digest: CheckpointContentsDigest::random(),
        previous_digest: Some(CheckpointDigest::random()),
        epoch_rolling_gas_cost_summary: GasCostSummary {
            computation_cost: 100,
            storage_cost: 50,
            storage_rebate: 5,
            non_refundable_storage_fee: 1,
        },
        timestamp_ms: 1234567890,
        checkpoint_commitments: vec![],
        end_of_epoch_data: None,
        version_specific_data: vec![],
    };

    // Serialize to BCS bytes
    let encoded = bcs::to_bytes(&checkpoint).expect("Failed to serialize to BCS");

    // Parse with sui-bcs to get Value
    let yaml_content = include_str!("../../../sui-core/tests/staged/sui.yaml");
    let parser = sui_bcs::Parser::from_yaml(yaml_content).expect("Failed to parse YAML");

    let value = parser
        .parse(&encoded, "CheckpointSummary")
        .expect("Failed to parse BCS data");

    // Convert Value back to Rust type
    let converted = CheckpointSummary::try_from(value).expect("Failed to convert from Value");

    // Check equality
    assert_eq!(checkpoint, converted, "Round-trip conversion failed");
}
