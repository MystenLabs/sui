// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::{
    base_types::{SuiAddress, random_object_ref},
    digests::{ChainIdentifier, CheckpointDigest},
    error::UserInputError,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{GasData, TransactionDataV1, TransactionExpiration, TransactionKind},
};
use sui_protocol_config::ProtocolConfig;

fn create_config_with_address_balance_gas_payments_enabled() -> ProtocolConfig {
    let mut config = ProtocolConfig::get_for_max_version_UNSAFE();
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
    let config = create_config_with_address_balance_gas_payments_enabled();

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
    let config = create_config_with_address_balance_gas_payments_enabled();

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
    let config = create_config_with_address_balance_gas_payments_enabled();

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
    let config = ProtocolConfig::get_for_max_version_UNSAFE();

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
    let config = ProtocolConfig::get_for_max_version_UNSAFE();

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
    let config = ProtocolConfig::get_for_max_version_UNSAFE();

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
