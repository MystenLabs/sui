// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::{
    base_types::{ObjectID, SequenceNumber, SuiAddress, random_object_ref},
    coin_reservation::ParsedObjectRefWithdrawal,
    digests::{ChainIdentifier, CheckpointDigest, ObjectDigest},
    error::UserInputError,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{
        CallArg, GasData, ObjectArg, SharedObjectMutability, TransactionDataV1,
        TransactionExpiration, TransactionKind,
    },
};
use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};

fn create_config_with_address_balance_gas_payments_enabled() -> ProtocolConfig {
    let mut config = ProtocolConfig::get_for_max_version_UNSAFE();
    config.enable_address_balance_gas_payments_for_testing();
    config
}

fn create_config_without_relax_valid_during() -> ProtocolConfig {
    // Use version 114 which has address balance gas but not the relax flag
    let mut config = ProtocolConfig::get_for_version(ProtocolVersion::new(114), Chain::Unknown);
    config.enable_address_balance_gas_payments_for_testing();
    config
}

fn create_test_transaction_data(
    gas_payment: Vec<ObjectRef>,
    expiration: TransactionExpiration,
) -> TransactionDataV1 {
    let sender = SuiAddress::random_for_testing_only();
    let builder = ProgrammableTransactionBuilder::new();
    let pt = builder.finish();

    TransactionDataV1 {
        kind: TransactionKind::ProgrammableTransaction(pt),
        sender,
        gas_data: GasData {
            payment: gas_payment,
            owner: sender,
            price: 1000,
            budget: 1000000,
        },
        expiration,
    }
}

fn create_address_balance_tx(
    price: u64,
    budget: u64,
    owner: Option<SuiAddress>,
) -> TransactionDataV1 {
    let sender = SuiAddress::random_for_testing_only();
    let gas_owner = owner.unwrap_or(sender);
    let builder = ProgrammableTransactionBuilder::new();
    let pt = builder.finish();

    TransactionDataV1 {
        kind: TransactionKind::ProgrammableTransaction(pt),
        sender,
        gas_data: GasData {
            payment: vec![],
            owner: gas_owner,
            price,
            budget,
        },
        expiration: TransactionExpiration::ValidDuring {
            min_epoch: Some(0),
            max_epoch: Some(0),
            min_timestamp: None,
            max_timestamp: None,
            chain: ChainIdentifier::from(CheckpointDigest::default()),
            nonce: 123,
        },
    }
}

fn create_test_transaction_data_with_owned_inputs(
    gas_payment: Vec<ObjectRef>,
    expiration: TransactionExpiration,
    owned_inputs: Vec<ObjectRef>,
) -> TransactionDataV1 {
    let sender = SuiAddress::random_for_testing_only();
    let mut builder = ProgrammableTransactionBuilder::new();
    for obj_ref in owned_inputs {
        builder
            .input(CallArg::Object(ObjectArg::ImmOrOwnedObject(obj_ref)))
            .unwrap();
    }
    let pt = builder.finish();

    TransactionDataV1 {
        kind: TransactionKind::ProgrammableTransaction(pt),
        sender,
        gas_data: GasData {
            payment: gas_payment,
            owner: sender,
            price: 1000,
            budget: 1000000,
        },
        expiration,
    }
}
#[test]
fn test_address_balance_payment_requires_accumulators_enabled() {
    let mut config = ProtocolConfig::get_for_max_version_UNSAFE();

    // accumulators not enabled
    config.disable_accumulators_for_testing();

    let tx_data = create_test_transaction_data(
        vec![],
        TransactionExpiration::ValidDuring {
            min_epoch: Some(0),
            max_epoch: Some(0),
            min_timestamp: None,
            max_timestamp: None,
            chain: ChainIdentifier::from(CheckpointDigest::default()),
            nonce: 123,
        },
    );

    let result = tx_data.validity_check(&TxValidityCheckContext::from_cfg_for_testing(&config));
    assert!(result.is_err());
    match result.unwrap_err().into_inner() {
        SuiErrorKind::UserInputError {
            error: UserInputError::MissingGasPayment,
        } => {}
        _ => panic!("Expected MissingGasPayment error for disabled accumulators"),
    }
}

#[test]
fn test_address_balance_payment_requires_feature_flag() {
    let mut config = ProtocolConfig::get_for_max_version_UNSAFE();
    config.enable_accumulators_for_testing();
    config.disable_address_balance_gas_payments_for_testing();

    let tx_data = create_test_transaction_data(
        vec![],
        TransactionExpiration::ValidDuring {
            min_epoch: Some(0),
            max_epoch: Some(0),
            min_timestamp: None,
            max_timestamp: None,
            chain: ChainIdentifier::from(CheckpointDigest::default()),
            nonce: 123,
        },
    );

    let result = tx_data.validity_check(&TxValidityCheckContext::from_cfg_for_testing(&config));
    assert!(result.is_err());
    match result.unwrap_err().into_inner() {
        SuiErrorKind::UserInputError {
            error: UserInputError::MissingGasPayment,
        } => {}
        _ => panic!("Expected MissingGasPayment error when feature flag is disabled"),
    }
}

#[test]
fn test_address_balance_payment_valid() {
    let config = create_config_with_address_balance_gas_payments_enabled();

    let tx_data = create_test_transaction_data(
        vec![],
        TransactionExpiration::ValidDuring {
            min_epoch: Some(0),
            max_epoch: Some(0),
            min_timestamp: None,
            max_timestamp: None,
            chain: ChainIdentifier::from(CheckpointDigest::default()),
            nonce: 123,
        },
    );

    let result = tx_data.validity_check(&TxValidityCheckContext::from_cfg_for_testing(&config));
    assert!(
        result.is_ok(),
        "Transaction should be valid with accumulators enabled"
    );
}

#[test]
fn test_address_balance_payment_requires_valid_during_expiration() {
    // When relax_valid_during_for_owned_inputs is disabled, validity_check
    // requires ValidDuring for all address balance gas payments.
    let config = create_config_without_relax_valid_during();

    let tx_data = create_test_transaction_data(vec![], TransactionExpiration::None);

    let result = tx_data.validity_check(&TxValidityCheckContext::from_cfg_for_testing(&config));
    assert!(result.is_err());
    match result.unwrap_err().into_inner() {
        SuiErrorKind::UserInputError {
            error: UserInputError::MissingGasPayment,
        } => {}
        _ => panic!("Expected MissingGasPayment error"),
    }

    let tx_data = create_test_transaction_data(vec![], TransactionExpiration::Epoch(1));

    let result = tx_data.validity_check(&TxValidityCheckContext::from_cfg_for_testing(&config));
    assert!(result.is_err());
    match result.unwrap_err().into_inner() {
        SuiErrorKind::UserInputError {
            error: UserInputError::InvalidExpiration { .. },
        } => {}
        _ => panic!("Expected InvalidExpiration error"),
    }
}

#[test]
fn test_address_balance_payment_single_epoch_validation() {
    let config = create_config_with_address_balance_gas_payments_enabled();

    let tx_data = create_test_transaction_data(
        vec![],
        TransactionExpiration::ValidDuring {
            min_epoch: Some(0),
            max_epoch: Some(0),
            min_timestamp: None,
            max_timestamp: None,
            chain: ChainIdentifier::from(CheckpointDigest::default()),
            nonce: 456,
        },
    );

    let result = tx_data.validity_check(&TxValidityCheckContext::from_cfg_for_testing(&config));
    assert!(result.is_ok(), "Single epoch expiration should be valid");
}

#[test]
fn test_address_balance_payment_one_epoch_range_validation() {
    let config = create_config_with_address_balance_gas_payments_enabled();

    let tx_data = create_test_transaction_data(
        vec![],
        TransactionExpiration::ValidDuring {
            min_epoch: Some(0),
            max_epoch: Some(1),
            min_timestamp: None,
            max_timestamp: None,
            chain: ChainIdentifier::from(CheckpointDigest::default()),
            nonce: 789,
        },
    );

    let result = tx_data.validity_check(&TxValidityCheckContext::from_cfg_for_testing(&config));
    assert!(
        result.is_ok(),
        "1-epoch range (min_epoch to min_epoch+1) should be valid"
    );
}

#[test]
fn test_address_balance_payment_multi_epoch_range_rejected() {
    // Use config without relax_valid_during_for_owned_inputs to test legacy behavior
    let config = create_config_without_relax_valid_during();

    let tx_data = create_test_transaction_data(
        vec![],
        TransactionExpiration::ValidDuring {
            min_epoch: Some(5),
            max_epoch: Some(7),
            min_timestamp: None,
            max_timestamp: None,
            chain: ChainIdentifier::from(CheckpointDigest::default()),
            nonce: 999,
        },
    );

    let result = tx_data.validity_check(&TxValidityCheckContext::from_cfg_for_testing(&config));
    assert!(result.is_err());
    match result.unwrap_err().into_inner() {
        SuiErrorKind::UserInputError {
            error: UserInputError::Unsupported(msg),
        } => {
            assert!(msg.contains("max_epoch must be at most min_epoch + 1"));
        }
        _ => panic!("Expected Unsupported error for epoch range > 1"),
    }
}

#[test]
fn test_address_balance_payment_timestamp_validation() {
    let config = create_config_with_address_balance_gas_payments_enabled();

    let tx_data = create_test_transaction_data(
        vec![],
        TransactionExpiration::ValidDuring {
            min_epoch: Some(0),
            max_epoch: Some(0),
            min_timestamp: Some(1000),
            max_timestamp: None,
            chain: ChainIdentifier::from(CheckpointDigest::default()),
            nonce: 999,
        },
    );

    let result = tx_data.validity_check(&TxValidityCheckContext::from_cfg_for_testing(&config));
    assert!(result.is_err());
    match result.unwrap_err().into_inner() {
        SuiErrorKind::UserInputError {
            error: UserInputError::Unsupported(msg),
        } => {
            assert!(msg.contains("Timestamp-based transaction expiration is not yet supported"));
        }
        _ => panic!("Expected Unsupported error for timestamp expiration"),
    }
}

#[test]
fn test_address_balance_payment_missing_epochs() {
    // Use config without relax_valid_during_for_owned_inputs to test legacy behavior
    let config = create_config_without_relax_valid_during();

    fn assert_missing_epoch_error(
        config: &ProtocolConfig,
        expiration: TransactionExpiration,
        case_description: &str,
    ) {
        let tx_data = create_test_transaction_data(vec![], expiration);

        let result = tx_data.validity_check(&TxValidityCheckContext::from_cfg_for_testing(config));
        assert!(result.is_err());
        match result.unwrap_err().into_inner() {
            SuiErrorKind::UserInputError {
                error: UserInputError::Unsupported(msg),
            } => {
                assert!(msg.contains("Both min_epoch and max_epoch must be specified"));
            }
            _ => panic!("Expected Unsupported error for {}", case_description),
        }
    }

    assert_missing_epoch_error(
        &config,
        TransactionExpiration::ValidDuring {
            min_epoch: None,
            max_epoch: None,
            min_timestamp: None,
            max_timestamp: None,
            chain: ChainIdentifier::from(CheckpointDigest::default()),
            nonce: 111,
        },
        "missing epochs",
    );
    assert_missing_epoch_error(
        &config,
        TransactionExpiration::ValidDuring {
            min_epoch: Some(1),
            max_epoch: None,
            min_timestamp: None,
            max_timestamp: None,
            chain: ChainIdentifier::from(CheckpointDigest::default()),
            nonce: 222,
        },
        "partial epoch specification (min only)",
    );
    assert_missing_epoch_error(
        &config,
        TransactionExpiration::ValidDuring {
            min_epoch: None,
            max_epoch: Some(1),
            min_timestamp: None,
            max_timestamp: None,
            chain: ChainIdentifier::from(CheckpointDigest::default()),
            nonce: 333,
        },
        "partial epoch specification (max only)",
    );
}

#[test]
fn test_regular_gas_payment_works_without_accumulators() {
    let config = ProtocolConfig::get_for_max_version_UNSAFE();

    let tx_data =
        create_test_transaction_data(vec![random_object_ref()], TransactionExpiration::None);

    let result = tx_data.validity_check(&TxValidityCheckContext::from_cfg_for_testing(&config));
    assert!(
        result.is_ok(),
        "Regular gas payment should work without accumulators"
    );
}

#[test]
fn test_regular_gas_payment_with_valid_during_expiration() {
    let config = ProtocolConfig::get_for_max_version_UNSAFE();

    let tx_data = create_test_transaction_data(
        vec![random_object_ref()],
        TransactionExpiration::ValidDuring {
            min_epoch: Some(0),
            max_epoch: Some(0),
            min_timestamp: None,
            max_timestamp: None,
            chain: ChainIdentifier::from(CheckpointDigest::default()),
            nonce: 123,
        },
    );

    let result = tx_data.validity_check(&TxValidityCheckContext::from_cfg_for_testing(&config));
    assert!(
        result.is_ok(),
        "Regular gas payment with ValidDuring expiration should be allowed"
    );
}

#[test]
fn test_regular_gas_payment_with_invalid_valid_during_timestamp() {
    let config = ProtocolConfig::get_for_max_version_UNSAFE();

    let tx_data = create_test_transaction_data(
        vec![random_object_ref()],
        TransactionExpiration::ValidDuring {
            min_epoch: Some(1),
            max_epoch: Some(1),
            min_timestamp: Some(1000),
            max_timestamp: None,
            chain: ChainIdentifier::from(CheckpointDigest::default()),
            nonce: 123,
        },
    );

    let result = tx_data.validity_check(&TxValidityCheckContext::from_cfg_for_testing(&config));
    assert!(result.is_err());
    match result.unwrap_err().into_inner() {
        SuiErrorKind::UserInputError {
            error: UserInputError::Unsupported(msg),
        } => {
            assert!(msg.contains("Timestamp-based transaction expiration is not yet supported"));
        }
        _ => panic!("Expected Unsupported error for timestamp expiration"),
    }
}

#[test]
fn test_regular_gas_payment_with_invalid_valid_during_multi_epoch() {
    // Use config without relax_valid_during_for_owned_inputs to test legacy behavior
    let config = create_config_without_relax_valid_during();

    let tx_data = create_test_transaction_data(
        vec![random_object_ref()],
        TransactionExpiration::ValidDuring {
            min_epoch: Some(1),
            max_epoch: Some(3),
            min_timestamp: None,
            max_timestamp: None,
            chain: ChainIdentifier::from(CheckpointDigest::default()),
            nonce: 123,
        },
    );

    let result = tx_data.validity_check(&TxValidityCheckContext::from_cfg_for_testing(&config));
    assert!(result.is_err());
    match result.unwrap_err().into_inner() {
        SuiErrorKind::UserInputError {
            error: UserInputError::Unsupported(msg),
        } => {
            assert!(msg.contains("max_epoch must be at most min_epoch + 1"));
        }
        _ => panic!("Expected Unsupported error for multi-epoch expiration"),
    }
}

#[test]
fn test_regular_gas_payment_with_invalid_valid_during_missing_epochs() {
    // Use config without relax_valid_during_for_owned_inputs to test legacy behavior
    let config = create_config_without_relax_valid_during();

    let tx_data = create_test_transaction_data(
        vec![random_object_ref()],
        TransactionExpiration::ValidDuring {
            min_epoch: None,
            max_epoch: None,
            min_timestamp: None,
            max_timestamp: None,
            chain: ChainIdentifier::from(CheckpointDigest::default()),
            nonce: 123,
        },
    );

    let result = tx_data.validity_check(&TxValidityCheckContext::from_cfg_for_testing(&config));
    assert!(result.is_err());
    match result.unwrap_err().into_inner() {
        SuiErrorKind::UserInputError {
            error: UserInputError::Unsupported(msg),
        } => {
            assert!(msg.contains("Both min_epoch and max_epoch must be specified"));
        }
        _ => panic!("Expected Unsupported error for missing epochs"),
    }
}

#[test]
fn test_regular_gas_payment_with_invalid_valid_during_partial_epochs() {
    // Use config without relax_valid_during_for_owned_inputs to test legacy behavior
    let config = create_config_without_relax_valid_during();

    let tx_data = create_test_transaction_data(
        vec![random_object_ref()],
        TransactionExpiration::ValidDuring {
            min_epoch: Some(1),
            max_epoch: None,
            min_timestamp: None,
            max_timestamp: None,
            chain: ChainIdentifier::from(CheckpointDigest::default()),
            nonce: 123,
        },
    );

    let result = tx_data.validity_check(&TxValidityCheckContext::from_cfg_for_testing(&config));
    assert!(result.is_err());
    match result.unwrap_err().into_inner() {
        SuiErrorKind::UserInputError {
            error: UserInputError::Unsupported(msg),
        } => {
            assert!(msg.contains("Both min_epoch and max_epoch must be specified"));
        }
        _ => panic!("Expected Unsupported error for partial epoch specification"),
    }
}

#[test]
fn test_regular_gas_payment_with_epoch_expiration() {
    let config = ProtocolConfig::get_for_max_version_UNSAFE();

    let tx_data =
        create_test_transaction_data(vec![random_object_ref()], TransactionExpiration::Epoch(5));

    let result = tx_data.validity_check(&TxValidityCheckContext::from_cfg_for_testing(&config));
    assert!(
        result.is_ok(),
        "Regular gas payment with Epoch expiration should be allowed"
    );
}

#[test]
fn test_address_balance_budget_zero_rejected() {
    let config = create_config_with_address_balance_gas_payments_enabled();
    let tx_data = create_address_balance_tx(1000, 0, None);

    let result = tx_data.validity_check(&TxValidityCheckContext::from_cfg_for_testing(&config));
    assert!(result.is_err());
    match result.unwrap_err().into_inner() {
        SuiErrorKind::UserInputError {
            error: UserInputError::GasBudgetTooLow { gas_budget: 0, .. },
        } => {}
        e => panic!("Expected GasBudgetTooLow, got: {:?}", e),
    }
}

#[test]
fn test_address_balance_gas_price_zero_rejected() {
    let config = create_config_with_address_balance_gas_payments_enabled();
    let tx_data = create_address_balance_tx(0, 1_000_000, None);

    let result = tx_data.validity_check(&TxValidityCheckContext::from_cfg_for_testing(&config));
    assert!(result.is_err());
    match result.unwrap_err().into_inner() {
        SuiErrorKind::UserInputError {
            error:
                UserInputError::GasPriceUnderRGP {
                    gas_price: 0,
                    reference_gas_price: 1000,
                },
        } => {}
        e => panic!("Expected GasPriceUnderRGP, got: {:?}", e),
    }
}

#[test]
fn test_address_balance_gas_price_below_rgp_rejected() {
    let config = create_config_with_address_balance_gas_payments_enabled();
    let tx_data = create_address_balance_tx(1, 1_000_000, None);

    let result = tx_data.validity_check(&TxValidityCheckContext::from_cfg_for_testing(&config));
    assert!(result.is_err());
    match result.unwrap_err().into_inner() {
        SuiErrorKind::UserInputError {
            error:
                UserInputError::GasPriceUnderRGP {
                    gas_price: 1,
                    reference_gas_price: 1000,
                },
        } => {}
        e => panic!("Expected GasPriceUnderRGP, got: {:?}", e),
    }
}

#[test]
fn test_address_balance_sponsored_budget_zero_rejected() {
    let config = create_config_with_address_balance_gas_payments_enabled();
    let sponsor = SuiAddress::random_for_testing_only();
    let tx_data = create_address_balance_tx(1000, 0, Some(sponsor));

    let result = tx_data.validity_check(&TxValidityCheckContext::from_cfg_for_testing(&config));
    assert!(result.is_err());
    match result.unwrap_err().into_inner() {
        SuiErrorKind::UserInputError {
            error: UserInputError::GasBudgetTooLow { gas_budget: 0, .. },
        } => {}
        e => panic!("Expected GasBudgetTooLow, got: {:?}", e),
    }
}

#[test]
fn test_address_balance_max_epoch_edge_case() {
    let config = create_config_with_address_balance_gas_payments_enabled();
    let sender = SuiAddress::random_for_testing_only();
    let builder = ProgrammableTransactionBuilder::new();
    let pt = builder.finish();

    let tx_data = TransactionDataV1 {
        kind: TransactionKind::ProgrammableTransaction(pt),
        sender,
        gas_data: GasData {
            payment: vec![],
            owner: sender,
            price: 1000,
            budget: 1_000_000,
        },
        expiration: TransactionExpiration::ValidDuring {
            min_epoch: Some(u64::MAX),
            max_epoch: Some(u64::MAX),
            min_timestamp: None,
            max_timestamp: None,
            chain: ChainIdentifier::from(CheckpointDigest::default()),
            nonce: 123,
        },
    };

    let context_at_max = TxValidityCheckContext {
        config: &config,
        epoch: u64::MAX,
        chain_identifier: ChainIdentifier::default(),
        reference_gas_price: 1000,
    };
    let result = tx_data.validity_check(&context_at_max);
    assert!(result.is_ok(), "Should not panic with u64::MAX epoch");

    let context_at_zero = TxValidityCheckContext::from_cfg_for_testing(&config);
    let result = tx_data.validity_check(&context_at_zero);
    assert!(result.is_err());
    match result.unwrap_err().into_inner() {
        SuiErrorKind::TransactionExpired => {}
        e => panic!("Expected TransactionExpired, got: {:?}", e),
    }
}

// Tests for relax_valid_during_for_owned_inputs feature

#[test]
fn test_address_balance_with_owned_inputs_allows_no_expiration() {
    let config = create_config_with_address_balance_gas_payments_enabled();

    let tx_data = create_test_transaction_data_with_owned_inputs(
        vec![],
        TransactionExpiration::None,
        vec![random_object_ref()],
    );

    let result = tx_data.validity_check(&TxValidityCheckContext::from_cfg_for_testing(&config));
    assert!(
        result.is_ok(),
        "Address balance gas with owned inputs should allow None expiration when flag enabled"
    );
}

#[test]
fn test_address_balance_with_owned_inputs_allows_epoch_expiration() {
    let config = create_config_with_address_balance_gas_payments_enabled();

    let tx_data = create_test_transaction_data_with_owned_inputs(
        vec![],
        TransactionExpiration::Epoch(5),
        vec![random_object_ref()],
    );

    let result = tx_data.validity_check(&TxValidityCheckContext::from_cfg_for_testing(&config));
    assert!(
        result.is_ok(),
        "Address balance gas with owned inputs should allow Epoch expiration when flag enabled"
    );
}

#[test]
fn test_address_balance_with_owned_inputs_allows_valid_during_expiration() {
    let config = create_config_with_address_balance_gas_payments_enabled();

    let tx_data = create_test_transaction_data_with_owned_inputs(
        vec![],
        TransactionExpiration::ValidDuring {
            min_epoch: Some(0),
            max_epoch: Some(0),
            min_timestamp: None,
            max_timestamp: None,
            chain: ChainIdentifier::from(CheckpointDigest::default()),
            nonce: 123,
        },
        vec![random_object_ref()],
    );

    let result = tx_data.validity_check(&TxValidityCheckContext::from_cfg_for_testing(&config));
    assert!(
        result.is_ok(),
        "Address balance gas with owned inputs should allow ValidDuring expiration"
    );
}

// Note: The test for stateless transactions requiring ValidDuring is now in
// sui-transaction-checks where we have access to actual object ownership.
// See check_address_balance_replay_protection() in sui-transaction-checks/src/lib.rs.

#[test]
fn test_address_balance_with_multiple_owned_inputs() {
    let config = create_config_with_address_balance_gas_payments_enabled();

    let tx_data = create_test_transaction_data_with_owned_inputs(
        vec![],
        TransactionExpiration::None,
        vec![
            random_object_ref(),
            random_object_ref(),
            random_object_ref(),
        ],
    );

    let result = tx_data.validity_check(&TxValidityCheckContext::from_cfg_for_testing(&config));
    assert!(
        result.is_ok(),
        "Address balance gas with multiple owned inputs should allow None expiration"
    );
}

fn create_coin_reservation_object_ref() -> ObjectRef {
    let withdrawal = ParsedObjectRefWithdrawal::new(ObjectID::random(), 0, 1000);
    (
        ObjectID::random(),
        SequenceNumber::from_u64(1),
        ObjectDigest::from(withdrawal.parsed_digest),
    )
}

#[test]
fn test_address_balance_with_shared_objects_and_coin_reservation_allows_relaxed_expiration() {
    let mut config = create_config_with_address_balance_gas_payments_enabled();
    config.enable_coin_reservation_for_testing();

    // Shared objects + coin reservation should allow relaxed expiration
    // because the coin reservation provides replay protection via epoch binding
    let sender = SuiAddress::random_for_testing_only();
    let mut builder = ProgrammableTransactionBuilder::new();
    builder
        .input(CallArg::Object(ObjectArg::SharedObject {
            id: ObjectID::random(),
            initial_shared_version: SequenceNumber::from_u64(1),
            mutability: SharedObjectMutability::Mutable,
        }))
        .unwrap();
    builder
        .input(CallArg::Object(ObjectArg::ImmOrOwnedObject(
            create_coin_reservation_object_ref(),
        )))
        .unwrap();
    let pt = builder.finish();

    let tx_data = TransactionDataV1 {
        kind: TransactionKind::ProgrammableTransaction(pt),
        sender,
        gas_data: GasData {
            payment: vec![],
            owner: sender,
            price: 1000,
            budget: 1000000,
        },
        expiration: TransactionExpiration::None,
    };

    let result = tx_data.validity_check(&TxValidityCheckContext::from_cfg_for_testing(&config));
    assert!(
        result.is_ok(),
        "Transaction with shared objects + coin reservation should allow None expiration"
    );
}
