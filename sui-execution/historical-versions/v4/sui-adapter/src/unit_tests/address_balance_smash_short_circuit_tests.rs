// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::execution_engine::checked::legacy::{
    ADDRESS_BALANCE_SMASH_SHORT_CIRCUIT_MIN_ACCUMULATOR_VERSION,
    should_short_circuit_insufficient_funds,
};
use nonempty::NonEmpty;
use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};
use sui_types::base_types::SequenceNumber;
use sui_types::execution_params::ExecutionOrEarlyError;
use sui_types::execution_status::ExecutionErrorKind;

/// Protocol version at which `early_exit_on_iffw` is enabled (the
/// version-126 arm in sui-protocol-config). The version one below it yields a config with
/// the flag still off. `flag_fixtures_match_protocol_gating` guards these against drift.
const FLAG_ACTIVATION_PROTOCOL_VERSION: u64 = 126;

fn config_with_flag() -> ProtocolConfig {
    ProtocolConfig::get_for_max_version_UNSAFE()
}

fn config_without_flag() -> ProtocolConfig {
    ProtocolConfig::get_for_version(
        ProtocolVersion::new(FLAG_ACTIVATION_PROTOCOL_VERSION - 1),
        Chain::Unknown,
    )
}

fn iffw(accumulator_version: Option<SequenceNumber>) -> ExecutionOrEarlyError {
    ExecutionOrEarlyError::failed(
        NonEmpty::new(ExecutionErrorKind::InsufficientFundsForWithdraw),
        accumulator_version,
    )
}

fn version(n: u64) -> Option<SequenceNumber> {
    Some(SequenceNumber::from_u64(n))
}

#[test]
fn flag_fixtures_match_protocol_gating() {
    // Anchor the version-based fixtures to the actual flag gating so the protocol-gating
    // tests below can't silently degrade if the activation version moves.
    assert!(config_with_flag().early_exit_on_iffw());
    assert!(!config_without_flag().early_exit_on_iffw());
}

#[test]
fn short_circuits_at_or_above_activation_version() {
    // At/above the settlement-version rollout point the version clause fires, so the
    // short-circuit holds whether or not the protocol flag is set.
    let activation = ADDRESS_BALANCE_SMASH_SHORT_CIRCUIT_MIN_ACCUMULATOR_VERSION.value();
    for config in [config_with_flag(), config_without_flag()] {
        assert!(should_short_circuit_insufficient_funds(
            &iffw(version(activation)),
            &config
        ));
        if let Some(next) = activation.checked_add(1) {
            assert!(should_short_circuit_insufficient_funds(
                &iffw(version(next)),
                &config
            ));
        }
    }
}

#[test]
fn preserves_hotfix_behavior_below_activation_version() {
    // In production builds: below the rollout point with the flag unset, no short-circuit.
    // In test/debug builds: `in_test_configuration()` fires and always short-circuits,
    // matching the ungated 1.72 mainnet hotfix to prevent fork scenarios in tests.
    let below = ADDRESS_BALANCE_SMASH_SHORT_CIRCUIT_MIN_ACCUMULATOR_VERSION.value() - 1;
    assert!(should_short_circuit_insufficient_funds(
        &iffw(version(below)),
        &config_without_flag()
    ));
}

#[test]
fn flag_forces_short_circuit_below_activation_version() {
    // Below the rollout point with the flag set (v126+): the version clause is false but the
    // flag clause carries it, so the short-circuit applies.
    let below = ADDRESS_BALANCE_SMASH_SHORT_CIRCUIT_MIN_ACCUMULATOR_VERSION.value() - 1;
    assert!(should_short_circuit_insufficient_funds(
        &iffw(version(below)),
        &config_with_flag()
    ));
}

#[test]
fn no_accumulator_version_short_circuits_in_test_configuration() {
    // In test/debug builds, IFFW with no accumulator version always short-circuits
    // (matches the ungated 1.72 mainnet hotfix, preventing fork scenarios in tests).
    // In production builds without the flag, this would return false — the mainnet
    // compiled-constant backfill requires an assigned accumulator version.
    assert!(should_short_circuit_insufficient_funds(
        &iffw(None),
        &config_without_flag(),
    ));
}

#[test]
fn no_accumulator_version_short_circuits_with_protocol_flag() {
    // Once the protocol flag is active, chains without accumulator versions should use the
    // new short-circuit behavior.
    assert!(should_short_circuit_insufficient_funds(
        &iffw(None),
        &config_with_flag(),
    ));
}

#[test]
fn iffw_short_circuit_applies_even_when_iffw_is_not_head_error() {
    // Intentional: once the short-circuit gate is active, any IFFW early error wins even
    // if another early error has higher/head priority.
    let errors = NonEmpty::from((
        ExecutionErrorKind::ExecutionCancelledDueToRandomnessUnavailable,
        vec![ExecutionErrorKind::InsufficientFundsForWithdraw],
    ));

    // Protocol-flag activation path, e.g. non-mainnet / no accumulator version.
    assert!(should_short_circuit_insufficient_funds(
        &ExecutionOrEarlyError::failed(errors.clone(), None),
        &config_with_flag(),
    ));

    // Mainnet compiled-constant activation path.
    assert!(should_short_circuit_insufficient_funds(
        &ExecutionOrEarlyError::failed(
            errors,
            version(ADDRESS_BALANCE_SMASH_SHORT_CIRCUIT_MIN_ACCUMULATOR_VERSION.value()),
        ),
        &config_without_flag(),
    ));
}

#[test]
fn non_head_iffw_short_circuits_in_test_configuration() {
    // In test/debug builds, any IFFW (even non-head) unconditionally short-circuits,
    // matching the ungated 1.72 mainnet hotfix.
    // In production builds without the flag or accumulator version, this would return
    // false — the non-head IFFW must not bypass the activation gate on its own.
    let errors = NonEmpty::from((
        ExecutionErrorKind::ExecutionCancelledDueToRandomnessUnavailable,
        vec![ExecutionErrorKind::InsufficientFundsForWithdraw],
    ));

    assert!(should_short_circuit_insufficient_funds(
        &ExecutionOrEarlyError::failed(errors, None),
        &config_without_flag(),
    ));
}

#[test]
fn requires_insufficient_funds_error() {
    // Only IFFW transactions short-circuit, regardless of accumulator version or whether
    // the protocol flag is set (the flag must never short-circuit a non-IFFW transaction).
    for config in [config_with_flag(), config_without_flag()] {
        assert!(!should_short_circuit_insufficient_funds(
            &ExecutionOrEarlyError::ok(version(
                ADDRESS_BALANCE_SMASH_SHORT_CIRCUIT_MIN_ACCUMULATOR_VERSION.value()
            )),
            &config
        ));
        assert!(!should_short_circuit_insufficient_funds(
            &ExecutionOrEarlyError::ok(None),
            &config
        ));
        assert!(!should_short_circuit_insufficient_funds(
            &ExecutionOrEarlyError::failed(
                NonEmpty::new(ExecutionErrorKind::CertificateDenied),
                version(ADDRESS_BALANCE_SMASH_SHORT_CIRCUIT_MIN_ACCUMULATOR_VERSION.value()),
            ),
            &config
        ));
        assert!(!should_short_circuit_insufficient_funds(
            &ExecutionOrEarlyError::failed(
                NonEmpty::new(ExecutionErrorKind::CertificateDenied),
                None,
            ),
            &config
        ));
    }
}
