// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui_system::validator_cap {
    use sui::object::{Self, ID, UID};
    use sui::transfer;
    use sui::tx_context::TxContext;
    friend sui_system::sui_system_state_inner;
    friend sui_system::validator;
    friend sui_system::validator_set;

    #[test_only]
    friend sui_system::sui_system_tests;
    #[test_only]
    friend sui_system::rewards_distribution_tests;

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
    struct UnverifiedValidatorOperationCap has key, store {
        id: UID,
        authorizer_validator_address: address,
    }

    /// Privileged operations require `ValidatorOperationCap` for permission check.
    /// This is only constructed after successful verification.
    struct ValidatorOperationCap has drop {
        authorizer_validator_address: address,
    }

    public(friend) fun unverified_operation_cap_address(cap: &UnverifiedValidatorOperationCap): &address {
        &cap.authorizer_validator_address
    }

    public(friend) fun verified_operation_cap_address(cap: &ValidatorOperationCap): &address {
        &cap.authorizer_validator_address
    }

    /// Should be only called by the friend modules when adding a `Validator`
    /// or rotating an existing validaotr's `operation_cap_id`.
    public(friend) fun new_unverified_validator_operation_cap_and_transfer(
        validator_address: address,
        ctx: &mut TxContext,
    ): ID {
        // MUSTFIX: update all tests to use @0x0 to create validators so we can
        // enforce the assert below.
        // This function needs to be called only by the validator itself, except
        // 1. in genesis where all valdiators are created by @0x0
        // 2. in tests where @0x0 could be used to simplify the setup
        // let sender_address = tx_context::sender(ctx);
        // assert!(sender_address == @0x0 || sender_address == validator_address, 0);

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
    public(friend) fun new_from_unverified(
        cap: &UnverifiedValidatorOperationCap,
    ): ValidatorOperationCap {
        ValidatorOperationCap {
            authorizer_validator_address: cap.authorizer_validator_address
        }
    }
}
