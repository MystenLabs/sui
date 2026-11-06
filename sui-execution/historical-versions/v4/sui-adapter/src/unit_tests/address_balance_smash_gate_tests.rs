// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::execution_engine::checked::legacy::{
    ADDRESS_BALANCE_SMASH_FIX_MIN_ACCUMULATOR_VERSION, should_filter_address_balance_gas_smash,
};
use nonempty::NonEmpty;
use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};
use sui_types::base_types::SequenceNumber;
use sui_types::execution_params::ExecutionOrEarlyError;
use sui_types::execution_status::ExecutionErrorKind;

/// The filter is only ever consulted with the `early_exit_on_iffw` flag off (a flag-on
/// IFFW short-circuits upstream), so the backfill gating is exercised against a flag-off
/// config. Protocol version 125 is one below the version-126 activation arm.
fn config_without_flag() -> ProtocolConfig {
    let config = ProtocolConfig::get_for_version(ProtocolVersion::new(125), Chain::Unknown);
    assert!(!config.early_exit_on_iffw());
    config
}

fn version(n: u64) -> Option<SequenceNumber> {
    Some(SequenceNumber::from_u64(n))
}

#[test]
fn applies_at_or_above_activation_version() {
    let activation = ADDRESS_BALANCE_SMASH_FIX_MIN_ACCUMULATOR_VERSION.value();
    for v in [activation, activation + 1] {
        assert!(should_filter_address_balance_gas_smash(
            &ExecutionOrEarlyError::failed(
                NonEmpty::new(ExecutionErrorKind::InsufficientFundsForWithdraw),
                version(v),
            ),
            &config_without_flag(),
        ));
    }
}

#[test]
fn preserves_old_behavior_below_activation_version() {
    // In production (non-test) builds, IFFW below the accumulator activation version
    // does not filter — the pre-flag hotfix behavior is preserved.
    // In test/debug builds `in_test_configuration()` fires first and the filter
    // always returns true to match the ungated 1.72 mainnet hotfix.
    let below = ADDRESS_BALANCE_SMASH_FIX_MIN_ACCUMULATOR_VERSION.value() - 1;
    assert!(should_filter_address_balance_gas_smash(
        &ExecutionOrEarlyError::failed(
            NonEmpty::new(ExecutionErrorKind::InsufficientFundsForWithdraw),
            version(below),
        ),
        &config_without_flag(),
    ));
}

#[test]
fn inert_without_accumulator_version() {
    // Non-IFFW early errors never filter, regardless of test configuration.
    let above = version(ADDRESS_BALANCE_SMASH_FIX_MIN_ACCUMULATOR_VERSION.value() + 1);
    assert!(!should_filter_address_balance_gas_smash(
        &ExecutionOrEarlyError::ok(above),
        &config_without_flag(),
    ));
    assert!(!should_filter_address_balance_gas_smash(
        &ExecutionOrEarlyError::failed(NonEmpty::new(ExecutionErrorKind::CertificateDenied), above),
        &config_without_flag(),
    ));
    // In test/debug builds, IFFW with no accumulator version returns true (matches
    // the ungated 1.72 mainnet hotfix). In production builds this would be false —
    // the mainnet backfill requires an assigned accumulator version.
    assert!(should_filter_address_balance_gas_smash(
        &ExecutionOrEarlyError::failed(
            NonEmpty::new(ExecutionErrorKind::InsufficientFundsForWithdraw),
            None,
        ),
        &config_without_flag(),
    ));
}
