// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::voting_power {
    use sui::validator::{Self, Validator};
    use std::vector;

    /// Set total_voting_power as 10_000 by convention. Individual voting powers can be interpreted
    /// as easily understandable basis points (e.g., voting_power: 100 = 1%, voting_power: 1 = 0.01%) rather than
    /// opaque quantities whose meaning changes from epoch to epoch as the total amount staked shifts.
    /// Fixing the total voting power allows clients to hardcode the quorum threshold and total_voting power rather
    /// than recomputing these.
    // TODO: we can go bigger if more precision is needed (e.g., will validators have <0.01% voting power given min stake requirements)?,
    // but I think using a power of 10 is a useful convention.
    const TOTAL_VOTING_POWER: u64 = 10_000;

    /// Quorum threshold for our fixed voting power--any message signed by this much voting power can be trusted
    /// up to BFT assumotions
    const QUORUM_THRESHOLD: u64 = 6_667;

    /// Cap voting power of an individual validator at 10%.
    // TODO: determine what this should be
    const MAX_VOTING_POWER: u64 = 1_000;

    /// We should never observe this, modulo bugs
    const EInternalInvariantViolation: u64 = 0;

    /// Convert each validator's stake to a voting power normalized w.r.t `TOTAL_VOTING_POWER`,
    /// and update `active_validators` accordingly, and attempt cap each validator's voting power at `MAX_VOTING_POWER`.
    /// Capping is handled by redistributing the voting power "taken away from" capped validators *proportionally* among validators with voting
    /// power less than the max.
    /// Similarly, "leftover" voting power due to rounding error is distributed *equally* among validators with voting power less than the max (if possible),
    /// and among all validators if everyone is already at the max.
    /// This function ensures the following invariants:
    /// 1. Total voting power of all validators sums to `TOTAL_VOTING_POWER`
    /// 2. `active_validators` is sorted by voting power in descending order
    /// 3. `active_validators` is sorted by stake in descending order
    /// This function attempts to maintain the following invariants whenever possible:
    /// 4. Each validator's voting power is <= `MAX_VOTING_POWER`
    /// 5. If validator A and B have the same stake, they will have the same voting power
    /// Invariant (4) and (5) should almost always hold for Sui in practice due to high validator count, and stakes that aren't exactly
    /// equal, but in theory/in tests these can be violated due to:
    /// - a staking distribution like [1, 1, 1, 1] (will violate (4))
    /// - a staking distribution where at least one validator has `MAX_VOTING_POWER`, and there is a remainder after redistributing the
    ///   leftovers due to odd numbers. In this case, a single validator will have 1 more than its proportionsal share
    /// Important note: after calling this function, *no* code that relies on indexes into `active_validators` should be used
    public fun update(active_validators: &mut vector<Validator>) {
        // sort validators by stake, in descending order
        bubble_sort_by_stake(active_validators);

        let total_stake = total_stake(active_validators);
        let voting_power_remaining = TOTAL_VOTING_POWER;
        let last_voting_power_remaining = voting_power_remaining;
        // index of the first validator with voting power < MAX_VOTING_POWER in `active_validators`
        // is always stable or increasing through the loop below
        let first_non_max_idx = 0;
        let num_validators = vector::length(active_validators);

        // zero out voting power
        let i = 0;
        while (i < num_validators) {
            let validator = vector::borrow_mut(active_validators, i);
            validator::set_voting_power(validator, 0);
            i = i + 1;
        };

        // this is do { ... } while (last_voting_power_remaining != voting_power_remaining),
        // but Move does not have do/while.
        // the loop terminates because voting_power_remaining is ever-decreasing, and will
        // eventually either reach 0 (if voting power can be evenly divided among validators
        // without violating the max constraint), or stabilize at `last_voting_power_remaining`
        // (if this is not possible)
        loop {
            let i = first_non_max_idx;
            // distribute voting power proportional to stake, but capping at MAX_VOTING_POWER
            while (i < num_validators) {
                let validator = vector::borrow_mut(active_validators, i);
                let validator_stake = validator::total_stake(validator);
                let prev_voting_power = validator::voting_power(validator);
                // note: multiplication before division here is important to minimize rounding error
                // multiplication can never overflow because validator stake is at most 50B * 10^9 (in
                // the absurd case where everything is staked with a single validator), and
                // last_voting_power_remaining is at most 10,000.
                let voting_power_share = (last_voting_power_remaining * validator_stake) / total_stake;
                let new_voting_power = prev_voting_power + voting_power_share;
                let voting_power_distributed = if (new_voting_power >= MAX_VOTING_POWER) {
                    validator::set_voting_power(validator, MAX_VOTING_POWER);
                    // new max validator--move index to the right
                    first_non_max_idx = i + 1;
                    voting_power_share - (new_voting_power - MAX_VOTING_POWER)
                } else {
                    validator::set_voting_power(validator, new_voting_power);
                    voting_power_share
                };
                voting_power_remaining = voting_power_remaining - voting_power_distributed;
                i = i + 1
            };
            check_intermediate_invariants(active_validators, voting_power_remaining, last_voting_power_remaining, first_non_max_idx);
            if (voting_power_remaining == 0 || voting_power_remaining == last_voting_power_remaining) { break };
            last_voting_power_remaining = voting_power_remaining
        };
        if (voting_power_remaining == 0) { return };
        // there is a remainder of voting power to be distributed. this can happen for two reasons:
        let i = if (first_non_max_idx == num_validators) {
            // reason 1: all validators have max voting power
            0
        } else {
            // reason 2: there is some leftover rounding error
            first_non_max_idx
        };
        let voting_power_share = voting_power_remaining / (num_validators - i);
        let remainder = voting_power_remaining % (num_validators - i);
        if (voting_power_share != 0) {
            while (i < num_validators) {
                let validator = vector::borrow_mut(active_validators, i);
                let prev_voting_power = validator::voting_power(validator);
                // this may be over the max, but we're ok with that. there's nowhere else to
                // put the excess voting power
                let new_voting_power = prev_voting_power + voting_power_share;
                validator::set_voting_power(validator, new_voting_power);
                if (new_voting_power >= MAX_VOTING_POWER) {
                    first_non_max_idx = i + 1
                };
                i = i + 1
            };
        };
        if (remainder == 0) { return };

        // if there's a remainder due to odd numbers, distribute 1 to each non-max validator until we run out,
        // if all validators are at max, distribute 1 to each validator until we run out.
        // this preserves the sorting invariant.
        let i = if (first_non_max_idx == num_validators) {
            // all validators have max voting power
            0
        } else {
            first_non_max_idx
        };
        // remainder should be small + not possible to evenly distribute to remaining validators
        assert!(remainder < num_validators - 1, EInternalInvariantViolation);
        while (remainder > 0 && i < num_validators) {
            let validator = vector::borrow_mut(active_validators, i);
            let prev_voting_power = validator::voting_power(validator);
            validator::set_voting_power(validator, prev_voting_power + 1);
            remainder = remainder - 1;
            i = i + 1
        };
        check_post_invariants(active_validators);
    }

    // TODO: use better sort, sticking with bubble sort here because it's simple
    /// Sort `v` in descending order by stake.
    fun bubble_sort_by_stake(v: &mut vector<Validator>) {
        let num_validators = vector::length(v);
        let max_stake = 18_446_744_073_709_551_615;
        loop {
            let i = 0;
            let last_stake = max_stake;
            let changed = false;
            while (i < num_validators) {
                let validator = vector::borrow(v, i);
                let validator_stake = validator::total_stake(validator);
                if (last_stake < validator_stake) {
                    vector::swap(v, i - 1, i);
                    changed = true
                };
                last_stake = validator_stake;
                i = i + 1
            };
            if (!changed) {
                return
            }
        }
    }

    /// Return the (constant) total voting power
    public fun total_voting_power(): u64 {
        TOTAL_VOTING_POWER
    }

    /// Return the (constant) quorum threshold
    public fun quorum_threshold(): u64 {
        QUORUM_THRESHOLD
    }

    /// Return the total stake of all validators in `v`
    public fun total_stake(v: &vector<Validator>): u64 {
        let i = 0;
        let len = vector::length(v);
        let total_stake = 0;
        while (i < len) {
            total_stake = total_stake + validator::total_stake(vector::borrow(v, i));
            i = i + 1
        };
        total_stake
    }

    /// Return the total voting power of all validators in `v`
    fun total_voting_powers(v: &vector<Validator>): u64 {
        let i = 0;
        let len = vector::length(v);
        let total_voting_power = 0;
        while (i < len) {
            total_voting_power = total_voting_power + validator::voting_power(vector::borrow(v, i));
            i = i + 1
        };
        total_voting_power
    }

    /// Check invariants that should hold on each each iteration of the proportional distribution loop
    fun check_intermediate_invariants(
        v: &vector<Validator>,
        voting_power_remaining: u64,
        last_voting_power_remaining: u64,
        first_non_max_idx: u64
    ) {
        // ensure we've conserved voting power
        assert!(total_voting_powers(v) + voting_power_remaining == TOTAL_VOTING_POWER, EInternalInvariantViolation);
        // ensure we're converging
        assert!(voting_power_remaining <= last_voting_power_remaining, EInternalInvariantViolation);
        // check that everything < first_non_max_idx has max voting power,
        // everything >= first_non_max_idx does not have max voting power.
        let i = 0;
        let num_validators = vector::length(v);
        while (i < num_validators) {
            let validator = vector::borrow(v, i);
            let voting_power = validator::voting_power(validator);
            if (i < first_non_max_idx) {
                assert!(voting_power >= MAX_VOTING_POWER, EInternalInvariantViolation);
            } else {
                assert!(voting_power < MAX_VOTING_POWER, EInternalInvariantViolation);
                // TODO: possible to check that voting power is proportional to stake?
            };
            i = i + 1
        }
    }

    /// check invariants that should hold after voting power assignment is complete
    fun check_post_invariants(v: &vector<Validator>) {
        // 1. Total voting power of all validators sums to `TOTAL_VOTING_POWER`
        assert!(total_voting_powers(v) == TOTAL_VOTING_POWER, EInternalInvariantViolation);
        // 2. `active_validators` is sorted by voting power in descending order
        // 3. `active_validators` is sorted by stake in descending order
        check_sorted(v);
    }

    /// Check that `v` is in descending order by both voting power and stake
    fun check_sorted(v: &vector<Validator>) {
        let num_validators = vector::length(v);
        let i = 0;
        let u64_max = 18_446_744_073_709_551_615;
        let last_stake = u64_max;
        let last_voting_power = u64_max;
        while (i < num_validators) {
            let validator = vector::borrow(v, i);
            let stake = validator::total_stake(validator);
            let voting_power = validator::voting_power(validator);
            assert!(last_stake >= stake, EInternalInvariantViolation);
            assert!(last_voting_power >= voting_power, EInternalInvariantViolation);
            last_stake = stake;
            last_voting_power = voting_power;
            i = i + 1
        }
    }

    /// Return the voting powers of `v` in sorted order
    public fun voting_power(v: &vector<Validator>): vector<u64> {
        let i = 0;
        let len = vector::length(v);
        let voting_power = vector[];
        while (i < len) {
            vector::push_back(&mut voting_power, validator::voting_power(vector::borrow(v, i)));
            i = i + 1
        };
        voting_power
    }

    #[test_only]
    public fun print_voting_power(v: &vector<Validator>) {
        let i = 0;
        let len = vector::length(v);
        while (i < len) {
            std::debug::print(&validator::voting_power(vector::borrow(v, i)));
            i = i + 1
        };
    }

    #[test_only]
    public fun print_stakes(v: &vector<Validator>) {
        let i = 0;
        let len = vector::length(v);
        while (i < len) {
            std::debug::print(&validator::total_stake(vector::borrow(v, i)));
            i = i + 1
        };
    }
}

