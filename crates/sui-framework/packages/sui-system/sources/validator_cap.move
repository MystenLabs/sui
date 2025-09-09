// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui_system::validator_cap;

/// The capability object is created when creating a new `Validator` or when the
/// validator explicitly creates a new capability object for rotation/revocation.
/// The holder address of this object can perform some validator operations on behalf of
/// the authorizer validator. Thus, if a validator wants to separate the keys for operation
/// (such as reference gas price setting or tallying rule reporting) from fund/staking, it
/// could transfer this capability object to another address.

/// To facilitate rotating/revocation, `Validator` stores the ID of currently valid
/// `UnverifiedValidatorOperationCap`. Thus, before converting `UnverifiedValidatorOperationCap`
/// to `ValidatorOperationCap`, verification needs to be done to make sure
/// the cap object is still valid.
public struct UnverifiedValidatorOperationCap has key, store {
    id: UID,
    authorizer_validator_address: address,
}

/// Privileged operations require `ValidatorOperationCap` for permission check.
/// This is only constructed after successful verification.
public struct ValidatorOperationCap has drop {
    authorizer_validator_address: address,
}

public(package) fun unverified_operation_cap_address(
    cap: &UnverifiedValidatorOperationCap,
): &address {
    &cap.authorizer_validator_address
}

public(package) fun verified_operation_cap_address(cap: &ValidatorOperationCap): &address {
    &cap.authorizer_validator_address
}

/// Should be only called by the friend modules when adding a `Validator`
/// or rotating an existing validaotr's `operation_cap_id`.
public(package) fun new_unverified_validator_operation_cap_and_transfer(
    validator_address: address,
    ctx: &mut TxContext,
): ID {
    // This function needs to be called only by the validator itself, except
    // 1. in genesis where all valdiators are created by @0x0
    // 2. in tests where @0x0 could be used to simplify the setup
    let sender_address = ctx.sender();
    assert!(sender_address == @0x0 || sender_address == validator_address, 0);

    let operation_cap = UnverifiedValidatorOperationCap {
        id: object::new(ctx),
        authorizer_validator_address: validator_address,
    };
    let operation_cap_id = object::id(&operation_cap);
    transfer::public_transfer(operation_cap, validator_address);
    operation_cap_id
}

/// Convert an `UnverifiedValidatorOperationCap` to `ValidatorOperationCap`.
/// Should only be called by `validator_set` module AFTER verification.
public(package) fun into_verified(cap: &UnverifiedValidatorOperationCap): ValidatorOperationCap {
    ValidatorOperationCap { authorizer_validator_address: cap.authorizer_validator_address }
}
