// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module frenemies::assignment {
    use std::vector;
    use sui::address;
    use sui::sui_system::{Self, SuiSystemState};
    use sui::tx_context::{Self, TxContext};
    use sui::validator_set;
    use sui::validator;

    /// An epoch assignment for a player in the game
    struct Assignment has store, drop, copy {
        /// validator this assignment is directed at
        validator: address,
        /// goal of the assignment: make validator one of {friend, enemy, neutral} at end of `epoch`
        goal: u8,
        /// epoch this assignment is for
        epoch: u64
    }

    /// Goal: validator finishes in top third by stake
    const FRIEND: u8 = 0;
    /// Goal: validator finishes in middle third by stake
    const NEUTRAL: u8 = 1;
    /// Goal: validator finishes in bottom third by stake
    const ENEMY: u8 = 2;

    /// Get this epoch's assignment for the current transaction sender
    /// Always returns the same assignment for the same tx sender and epoch.
    public fun get(state: &SuiSystemState, ctx: &TxContext): Assignment {
        let validators = validator_set::active_validators(sui_system::validators(state));
        let len = vector::length(validators);

        // derive validator + goal from the user's address and the current epoch.
        // a given address will always get the same assignment in the same epoch
        // goal assignments "round-robin" in that once the initial assignment
        // is known, the rest of the assignments are predictable
        let addr = address::to_u256(tx_context::sender(ctx));
        let epoch = tx_context::epoch(ctx);
        let assignment_seed = ((addr + (epoch as u256)) as u256);
        let validator_idx = assignment_seed % (len as u256);
        let validator = validator::sui_address(vector::borrow(validators, (validator_idx as u64)));
        let goal = get_goal(addr, epoch);
        Assignment { validator, goal, epoch }
    }

    fun get_goal(addr: u256, epoch: u64): u8 {
        // goals are round-robin, but different addresses will start at different points in the round-robin cycle
        let offset_epoch = addr + (epoch as u256);
        ((offset_epoch % 3) as u8)
    }

    /// Return `FRIEND` if `rank_idx` is in the top third, `NEUTRAL` if `rank_idx` is in the middle third,
    /// and `ENEMY` if `rank_idx` is in the bottom third in a system with `num_validators` validators.
    /// Note: `rank_idx` is a 0-indexed value, it should always be strictly less than `num_validators`.
    /// Note: because of truncating division, the "enemy" slice is bigger when validators is not divisible by 3
    public fun get_outcome(rank_idx: u64, num_validators: u64): u8 {
        if (rank_idx < num_validators / 3) {
            FRIEND // friend = top third
        } else if (rank_idx >= num_validators * 2 / 3) {
            ENEMY // enemy = bottom third
        } else {
            NEUTRAL // neutral = middle third
        }
    }

    public fun validator(self: &Assignment): address {
        self.validator
    }

    public fun goal(self: &Assignment): u8 {
        self.goal
    }

    public fun epoch(self: &Assignment): u64 {
        self.epoch
    }

    #[test_only]
    public fun new_for_testing(validator: address, goal: u8, epoch: u64): Assignment {
        Assignment { validator, goal, epoch }
    }

    #[test]
    fun test_outcomes() {
        assert!(get_outcome(0, 3) == FRIEND, 0);
        assert!(get_outcome(1, 3) == NEUTRAL, 0);
        assert!(get_outcome(2, 3) == ENEMY, 0);

        // note: when # of validators is not divisible by 3, outcomes skew toward ENEMY because of truncating division
        assert!(get_outcome(0, 4) == FRIEND, 0);
        assert!(get_outcome(1, 4) == NEUTRAL, 0);
        assert!(get_outcome(2, 4) == ENEMY, 0);
        assert!(get_outcome(3, 4) == ENEMY, 0);

        assert!(get_outcome(0, 5) == FRIEND, 0);
        assert!(get_outcome(1, 5) == NEUTRAL, 0);
        assert!(get_outcome(2, 5) == NEUTRAL, 0);
        assert!(get_outcome(3, 5) == ENEMY, 0);
        assert!(get_outcome(4, 5) == ENEMY, 0);

        assert!(get_outcome(0, 6) == FRIEND, 0);
        assert!(get_outcome(1, 6) == FRIEND, 0);
        assert!(get_outcome(2, 6) == NEUTRAL, 0);
        assert!(get_outcome(3, 6) == NEUTRAL, 0);
        assert!(get_outcome(4, 6) == ENEMY, 0);
        assert!(get_outcome(5, 6) == ENEMY, 0);

        assert!(get_outcome(0, 7) == FRIEND, 0);
        assert!(get_outcome(1, 7) == FRIEND, 0);
        assert!(get_outcome(2, 7) == NEUTRAL, 0);
        assert!(get_outcome(3, 7) == NEUTRAL, 0);
        assert!(get_outcome(4, 7) == ENEMY, 0);
        assert!(get_outcome(5, 7) == ENEMY, 0);
        assert!(get_outcome(6, 7) == ENEMY, 0);
    }

    #[test]
    fun round_robin_goal() {
        // an address should see a cycle of FRIEND, NEUTRAL, ENEMY goals
        let addr = address::to_u256(@0x0);
        assert!(get_goal(addr, 0) == FRIEND, 0);
        assert!(get_goal(addr, 1) == NEUTRAL, 0);
        assert!(get_goal(addr, 2) == ENEMY, 0);
        assert!(get_goal(addr, 3) == FRIEND, 0);
        assert!(get_goal(addr, 4) == NEUTRAL, 0);
        assert!(get_goal(addr, 5) == ENEMY, 0);

        // different addresses may start in a different place in the cycle
        let addr = address::to_u256(@0x1);
        assert!(get_goal(addr, 0) == NEUTRAL, 0);
        assert!(get_goal(addr, 1) == ENEMY, 0);
        assert!(get_goal(addr, 2) == FRIEND, 0);
        assert!(get_goal(addr, 3) == NEUTRAL, 0);
        assert!(get_goal(addr, 4) == ENEMY, 0);
        assert!(get_goal(addr, 5) == FRIEND, 0);
    }
}
