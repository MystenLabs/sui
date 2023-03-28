// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui_system::voting_power {
    use sui_system::validator::Validator;
    use std::vector;
    use sui_system::validator;
    use sui::math;
    use sui::math::divide_and_round_up;

    friend sui_system::validator_set;

    #[test_only]
    friend sui_system::voting_power_tests;

    struct VotingPowerInfo has drop {
        validator_index: u64,
        voting_power: u64,
    }

    /// Set total_voting_power as 10_000 by convention. Individual voting powers can be interpreted
    /// as easily understandable basis points (e.g., voting_power: 100 = 1%, voting_power: 1 = 0.01%) rather than
    /// opaque quantities whose meaning changes from epoch to epoch as the total amount staked shifts.
    /// Fixing the total voting power allows clients to hardcode the quorum threshold and total_voting power rather
    /// than recomputing these.
    const TOTAL_VOTING_POWER: u64 = 10_000;

    /// Quorum threshold for our fixed voting power--any message signed by this much voting power can be trusted
    /// up to BFT assumptions
    const QUORUM_THRESHOLD: u64 = 6_667;

    // Cap voting power of an individual validator at 10%.
    // TODO: determine what this should be
    const MAX_VOTING_POWER: u64 = 1_000;

    const ETotalPowerMismatch: u64 = 1;
    const ERelativePowerMismatch: u64 = 2;
    const EVotingPowerOverThreshold: u64 = 3;
    const EInvalidVotingPower: u64 = 4;

    /// Set the voting power of all validators.
    /// Each validator's voting power is initialized using their stake. We then attempt to cap their voting power
    /// at `MAX_VOTING_POWER`. If `MAX_VOTING_POWER` is not a feasible cap, we pick the lowest possible cap.
    public(friend) fun set_voting_power(validators: &mut vector<Validator>) {
        // If threshold_pct is too small, it's possible that even when all validators reach the threshold we still don't
        // have 100%. So we bound the threshold_pct to be always enough to find a solution.
        let threshold = math::min(
            TOTAL_VOTING_POWER,
            math::max(MAX_VOTING_POWER, divide_and_round_up(TOTAL_VOTING_POWER, vector::length(validators))),
        );
        let (info_list, remaining_power) = init_voting_power_info(validators, threshold);
        adjust_voting_power(&mut info_list, threshold, remaining_power);
        update_voting_power(validators, info_list);
        check_invariants(validators);
    }

    /// Create the initial voting power of each validator, set using their stake, but capped using threshold.
    /// We also perform insertion sort while creating the voting power list, by maintaining the list in
    /// descending order using voting power.
    /// Anything beyond the threshold is added to the remaining_power, which is also returned.
    fun init_voting_power_info(
        validators: &vector<Validator>,
        threshold: u64,
    ): (vector<VotingPowerInfo>, u64) {
        let total_stake = total_stake(validators);
        let i = 0;
        let len = vector::length(validators);
        let total_power = 0;
        let result = vector[];
        while (i < len) {
            let validator = vector::borrow(validators, i);
            let stake = validator::total_stake(validator);
            let adjusted_stake = (stake as u128) * (TOTAL_VOTING_POWER as u128) / (total_stake as u128);
            let voting_power = math::min((adjusted_stake as u64), threshold);
            let info = VotingPowerInfo {
                validator_index: i,
                voting_power,
            };
            insert(&mut result, info);
            total_power = total_power + voting_power;
            i = i + 1;
        };
        (result, TOTAL_VOTING_POWER - total_power)
    }

    /// Sum up the total stake of all validators.
    fun total_stake(validators: &vector<Validator>): u64 {
        let i = 0;
        let len = vector::length(validators);
        let total_stake =0 ;
        while (i < len) {
            total_stake = total_stake + validator::total_stake(vector::borrow(validators, i));
            i = i + 1;
        };
        total_stake
    }

    /// Insert `new_info` to `info_list` as part of insertion sort, such that `info_list` is always sorted
    /// using voting_power, in descending order.
    fun insert(info_list: &mut vector<VotingPowerInfo>, new_info: VotingPowerInfo) {
        let i = 0;
        let len = vector::length(info_list);
        while (i < len && vector::borrow(info_list, i).voting_power > new_info.voting_power) {
            i = i + 1;
        };
        vector::insert(info_list, new_info, i);
    }

    /// Distribute remaining_power to validators that are not capped at threshold.
    fun adjust_voting_power(info_list: &mut vector<VotingPowerInfo>, threshold: u64, remaining_power: u64) {
        let i = 0;
        let len = vector::length(info_list);
        while (i < len && remaining_power > 0) {
            let v = vector::borrow_mut(info_list, i);
            // planned is the amount of extra power we want to distribute to this validator.
            let planned = divide_and_round_up(remaining_power, len - i);
            // target is the targeting power this validator will reach, capped by threshold.
            let target = math::min(threshold, v.voting_power + planned);
            // actual is the actual amount of power we will be distributing to this validator.
            let actual = math::min(remaining_power, target - v.voting_power);
            v.voting_power = v.voting_power + actual;
            assert!(v.voting_power <= threshold, EVotingPowerOverThreshold);
            remaining_power = remaining_power - actual;
            i = i + 1;
        };
        assert!(remaining_power == 0, ETotalPowerMismatch);
    }

    /// Update validators with the decided voting power.
    fun update_voting_power(validators: &mut vector<Validator>, info_list: vector<VotingPowerInfo>) {
        while (!vector::is_empty(&info_list)) {
            let VotingPowerInfo {
                validator_index,
                voting_power,
            } = vector::pop_back(&mut info_list);
            let v = vector::borrow_mut(validators, validator_index);
            validator::set_voting_power(v, voting_power);
        };
        vector::destroy_empty(info_list);
    }

    /// Check a few invariants that must hold after setting the voting power.
    fun check_invariants(v: &vector<Validator>) {
        // First check that the total voting power must be TOTAL_VOTING_POWER.
        let i = 0;
        let len = vector::length(v);
        let total = 0;
        while (i < len) {
            let voting_power = validator::voting_power(vector::borrow(v, i));
            assert!(voting_power > 0, EInvalidVotingPower);
            total = total + voting_power;
            i = i + 1;
        };
        assert!(total == TOTAL_VOTING_POWER, ETotalPowerMismatch);

        // Second check that if validator A's stake is larger than B's stake, A's voting power must be no less
        // than B's voting power; similarly, if A's stake is less than B's stake, A's voting power must be no larger
        // than B's voting power.
        let a = 0;
        while (a < len) {
            let b = a + 1;
            while (b < len) {
                let validator_a = vector::borrow(v, a);
                let validator_b = vector::borrow(v, b);
                let stake_a = validator::total_stake(validator_a);
                let stake_b = validator::total_stake(validator_b);
                let power_a = validator::voting_power(validator_a);
                let power_b = validator::voting_power(validator_b);
                if (stake_a > stake_b) {
                    assert!(power_a >= power_b, ERelativePowerMismatch);
                };
                if (stake_a < stake_b) {
                    assert!(power_a <= power_b, ERelativePowerMismatch);
                };
                b = b + 1;
            };
            a = a + 1;
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
}
