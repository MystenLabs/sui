// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui_system::validator_wrapper;

use sui::versioned::{Self, Versioned};
use sui_system::staking_pool::StakingPool;
use sui_system::validator::Validator;

const EInvalidVersion: u64 = 0;

public struct ValidatorWrapper has store {
    inner: Versioned,
}

// Validator corresponds to version 1.
public(package) fun create_v1(validator: Validator, ctx: &mut TxContext): ValidatorWrapper {
    ValidatorWrapper {
        inner: versioned::create(1, validator, ctx),
    }
}

/// This function should always return the latest supported version.
/// If the inner version is old, we upgrade it lazily in-place.
public(package) fun load_validator_maybe_upgrade(self: &mut ValidatorWrapper): &mut Validator {
    self.upgrade_to_latest();
    self.inner.load_value_mut()
}

/// Destroy the wrapper and retrieve the inner validator object.
public(package) fun destroy(self: ValidatorWrapper): Validator {
    upgrade_to_latest(&self);
    let ValidatorWrapper { inner } = self;
    inner.destroy()
}

#[test_only]
/// Load the inner validator with assumed type. This should be used for testing only.
public(package) fun get_inner_validator_ref(self: &ValidatorWrapper): &Validator {
    self.inner.load_value()
}

/// Borrow the staking pool without upgrading the wrapper. Inactive and candidate entries are
/// upgraded only when a mutable path loads them, so this must read every stored version.
public(package) fun staking_pool_ref(self: &ValidatorWrapper): &StakingPool {
    let validator: &Validator = self.inner.load_value();
    validator.get_staking_pool_ref()
}

fun upgrade_to_latest(self: &ValidatorWrapper) {
    let version = self.version();
    // TODO: When new versions are added, we need to explicitly upgrade here.
    assert!(version == 1, EInvalidVersion);
}

fun version(self: &ValidatorWrapper): u64 {
    self.inner.version()
}
