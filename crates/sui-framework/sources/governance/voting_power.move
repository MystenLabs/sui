// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::voting_power {
    use sui::validator::Validator;
    use std::vector;
    use sui::validator;
    use sui::math;

    struct VotingPowerInfo {
        validator_index: u64,
        voting_power: u64,
    }

    const TOTAL_VOTING_POWER: u64 = 10000;

    const QUORUM_THRESHOLD: u64 = 6667;

    const MAX_VOTING_POWER: u64 = 1000;

    /// Set the voting power of all validators. The total stake of all validators is provided in `total_stake`.
    /// threshold_pct is a percentage threshold of max voting power that we want to cap on. If threshold_pct is 10,
    /// then we want to cap the voting power at 10%.
    public fun set_voting_power(validators: &mut vector<Validator>, total_stake: u64) {
        // If threshold_pct is too small, it's possible that even when all validators reach the threshold we still don't
        // have 100%. So we bound the threshold_pct to be always enough to find a solution.
        let threshold = math::min(
            TOTAL_VOTING_POWER,
            math::max(MAX_VOTING_POWER, TOTAL_VOTING_POWER / vector::length(validators) + 1),
        );
        let (info_list, remaining_power) = init_voting_power_info(validators, total_stake, threshold);
        bubble_sort(&mut info_list);
        adjust_voting_power(&mut info_list, threshold, remaining_power);
        update_voting_power(validators, info_list);
    }

    /// Create the initial voting power of each validator, set using their stake, but capped using threshold.
    /// Anything beyond the threshold is added to the remaining_power, which is also returned.
    fun init_voting_power_info(
        validators: &vector<Validator>,
        total_stake: u64,
        threshold: u64,
    ): (vector<VotingPowerInfo>, u64) {
        let i = 0;
        let len = vector::length(validators);
        let total_power = 0;
        let result = vector[];
        while (i < len) {
            let validator = vector::borrow(validators, i);
            let stake = validator::total_stake(validator);
            let adjusted_stake = stake as u128 * (TOTAL_VOTING_POWER as u128) / (total_stake as u128);
            let voting_power = math::min(adjusted_stake as u64, threshold);
            let info = VotingPowerInfo {
                validator_index: i,
                voting_power,
            };
            vector::push_back(&mut result, info);
            total_power = total_power + voting_power;
            i = i + 1;
        };
        (result, TOTAL_VOTING_POWER - total_power)
    }

    /// Sort the voting power info list, using the voting power, in descending order.
    fun bubble_sort(info_list: &mut vector<VotingPowerInfo>) {
        let len = vector::length(info_list);
        let changed = true;
        while (changed) {
            changed = false;
            let i = 0;
            while (i + 1 < len) {
                if (vector::borrow(info_list, i).voting_power < vector::borrow(info_list, i + 1).voting_power) {
                    changed = true;
                    vector::swap(info_list, i, i + 1);
                };
                i = i + 1;
            }
        }
    }

    /// Distribute remaining_power to validators that are not capped at threshold.
    fun adjust_voting_power(info_list: &mut vector<VotingPowerInfo>, threshold: u64, remaining_power: u64) {
        let i = 0;
        let len = vector::length(info_list);
        while (i < len && remaining_power > 0) {
            let v = vector::borrow_mut(info_list, i);
            let planned = remaining_power / (len - i) + 1;
            let target = math::min(threshold, v.voting_power + planned);
            let actural = math::min(remaining_power, target - v.voting_power);
            v.voting_power = v.voting_power + actural;
            remaining_power = remaining_power - actural;
            assert!(v.voting_power <= threshold, 0);
            i = i + 1;
        };
        assert!(remaining_power == 0, 0);
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
}
