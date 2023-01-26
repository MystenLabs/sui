// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Module that helps handling multiple delegation / withdrawal
/// requests. Helps doing programmable batches with the SuiSystem
/// object avoiding the transaction batch limitation (can't use the
/// same object twice).
module frenemies::delegation {
    use sui::sui_system::{Self, SuiSystemState};
    use sui::staking_pool::{Delegation, StakedSui};
    use sui::tx_context::TxContext;
    use std::vector;

    /// For when there's a mismatch between Delegation and StakedSui vectors.
    const EVecArgumentLengthMismatch: u64 = 0;

    /// Switch multiple delegations into a single Validator account.
    /// Vector of Delegations must match the vector of StakedSui objects.
    ///
    /// Aborts if there's a vector length mismatch.
    public entry fun switch_into_one(
        self: &mut SuiSystemState,
        delegations: vector<Delegation>,
        staked_suis: vector<StakedSui>, // don't mind if I do
        new_validator_address: address,
        ctx: &mut TxContext
    ) {
        let len = vector::length(&delegations);
        assert!(len == vector::length(&staked_suis), EVecArgumentLengthMismatch);

        while (len > 0) {
            let (staked_sui, delegation) = (
                vector::pop_back(&mut staked_suis),
                vector::pop_back(&mut delegations)
            );

            sui_system::request_switch_delegation(self, delegation, staked_sui, new_validator_address, ctx);
            len = len - 1;
        };

        vector::destroy_empty(delegations);
        vector::destroy_empty(staked_suis);
    }

    /// Request multiple withdraws at once.
    /// Vector of Delegations must match the vector of StakedSui objects.
    ///
    /// Aborts if there's a vector length mismatch.
    public entry fun request_withdraw_mul(
        self: &mut SuiSystemState,
        delegations: vector<Delegation>,
        staked_suis: vector<StakedSui>,
        ctx: &mut TxContext
    ) {
        let len = vector::length(&delegations);
        assert!(len == vector::length(&staked_suis), EVecArgumentLengthMismatch);

        while (len > 0) {
            let (staked_sui, delegation) = (
                vector::pop_back(&mut staked_suis),
                vector::pop_back(&mut delegations)
            );

            sui_system::request_withdraw_delegation(self, delegation, staked_sui, ctx);
            len = len - 1;
        };

        vector::destroy_empty(delegations);
        vector::destroy_empty(staked_suis);
    }
}
