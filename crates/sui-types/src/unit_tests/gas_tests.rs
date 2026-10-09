// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::error::{SuiErrorKind, SuiResult, UserInputError};
use crate::gas_model::gas_predicates::check_for_gas_price_too_high;
use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};

fn user_input_error(result: SuiResult<SuiGasStatus>) -> UserInputError {
    match result.unwrap_err().into_inner() {
        SuiErrorKind::UserInputError { error } => error,
        e => panic!("expected UserInputError, got {e:?}"),
    }
}

#[test]
fn test_zero_gas_price_is_rejected() {
    for config in [
        ProtocolConfig::get_for_version(ProtocolVersion::new(1), Chain::Unknown),
        ProtocolConfig::get_for_max_version_UNSAFE(),
    ] {
        let err = user_input_error(SuiGasStatus::new(1_000_000, 0, 0, &config));
        assert!(matches!(err, UserInputError::Unsupported(_)), "{err:?}");
    }
}

#[test]
fn test_uncapped_gas_price_overflow_is_rejected() {
    let config = ProtocolConfig::get_for_version(ProtocolVersion::new(1), Chain::Unknown);
    assert!(!check_for_gas_price_too_high(config.gas_model_version()));
    let limit = u64::MAX / config.max_gas_computation_bucket();

    SuiGasStatus::new(u64::MAX, limit, 1, &config).unwrap();
    for price in [limit + 1, u64::MAX] {
        let err = user_input_error(SuiGasStatus::new(u64::MAX, price, 1, &config));
        assert_eq!(
            err,
            UserInputError::GasPriceTooHigh {
                max_gas_price: limit
            }
        );
    }
}
