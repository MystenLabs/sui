// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::validator_set {
    use std::vector;

    use sui::tx_context::TxContext;
    use sui::validator::Validator;
    use sui::object::ID;
    use sui::vec_map::{Self, VecMap};
    use sui::table::{Self, Table};
    use sui::table_vec::{Self, TableVec};
    use sui::validator_wrapper::ValidatorWrapper;
    use sui::bag::Bag;
    use sui::bag;

    friend sui::genesis;
    friend sui::sui_system_state_inner;

    struct ValidatorSet has store {
        /// Total amount of stake from all active validators at the beginning of the epoch.
        total_stake: u64,

        /// The current list of active validators.
        active_validators: vector<Validator>,

        /// List of new validator candidates added during the current epoch.
        /// They will be processed at the end of the epoch.
        pending_active_validators: TableVec<Validator>,

        /// Removal requests from the validators. Each element is an index
        /// pointing to `active_validators`.
        pending_removals: vector<u64>,

        /// Mappings from staking pool's ID to the sui address of a validator.
        staking_pool_mappings: Table<ID, address>,

        /// Mapping from a staking pool ID to the inactive validator that has that pool as its staking pool.
        /// When a validator is deactivated the validator is removed from `active_validators` it
        /// is added to this table so that stakers can continue to withdraw their stake from it.
        inactive_validators: Table<ID, ValidatorWrapper>,

        /// Table storing preactive validators, mapping their addresses to their `Validator ` structs.
        /// When an address calls `request_add_validator_candidate`, they get added to this table and become a preactive
        /// validator.
        /// When the candidate has met the min stake requirement, they can call `request_add_validator` to
        /// officially add them to the active validator set `active_validators` next epoch.
        validator_candidates: Table<address, ValidatorWrapper>,

        /// Table storing the number of epochs during which a validator's stake has been below the low stake threshold.
        at_risk_validators: VecMap<address, u64>,

        /// Any extra fields that's not defined statically.
        extra_fields: Bag,
    }

    // ==== initialization at genesis ====

    public(friend) fun new(init_active_validators: vector<Validator>, ctx: &mut TxContext): ValidatorSet {
        ValidatorSet {
            total_stake: 0, // total_stake should not matter to run a bare-minimum protocol
            active_validators: init_active_validators,
            pending_active_validators: table_vec::empty(ctx),
            pending_removals: vector::empty(),
            staking_pool_mappings: table::new(ctx),
            inactive_validators: table::new(ctx),
            validator_candidates: table::new(ctx),
            at_risk_validators: vec_map::empty(),
            extra_fields: bag::new(ctx),
        }
    }
}
