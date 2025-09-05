// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui_system::voting_power;

use sui_system::validator::Validator;

#[allow(unused_field)]
/// Deprecated. Use VotingPowerInfoV2 instead.
public struct VotingPowerInfo has drop {
    validator_index: u64,
    voting_power: u64,
}

public struct VotingPowerInfoV2 has drop {
    validator_index: u64,
    voting_power: u64,
    stake: u64,
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
public(package) fun set_voting_power(validators: &mut vector<Validator>, total_stake: u64) {
    // If threshold_pct is too small, it's possible that even when all validators reach the threshold we still don't
    // have 100%. So we bound the threshold_pct to be always enough to find a solution.
    let total_voting_power = TOTAL_VOTING_POWER;
    let average_voting_power = total_voting_power.divide_and_round_up(validators.length());
    let threshold = total_voting_power.min(MAX_VOTING_POWER.max(average_voting_power));
    let (mut info_list, remaining_power) = init_voting_power_info(
        validators,
        threshold,
        total_stake,
    );
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
    total_stake: u64,
): (vector<VotingPowerInfoV2>, u64) {
    let mut total_power = 0;
    let mut result = vector[];
    validators.length().do!(|i| {
        let stake = validators[i].total_stake();
        let voting_power = derive_raw_voting_power(stake, total_stake).min(threshold);
        insert(&mut result, VotingPowerInfoV2 { validator_index: i, voting_power, stake });
        total_power = total_power + voting_power;
    });

    (result, TOTAL_VOTING_POWER - total_power)
}

public(package) fun derive_raw_voting_power(stake: u64, total_stake: u64): u64 {
    ((stake as u128 * (TOTAL_VOTING_POWER as u128) / (total_stake as u128)) as u64)
}

/// Insert `new_info` to `info_list` as part of insertion sort, such that `info_list` is always sorted
/// using stake, in descending order.
fun insert(info_list: &mut vector<VotingPowerInfoV2>, new_info: VotingPowerInfoV2) {
    let len = info_list.length();
    let idx = info_list.find_index!(|info| new_info.stake >= info.stake);
    info_list.insert(new_info, idx.destroy_or!(len));
}

/// Distribute remaining_power to validators that are not capped at threshold.
fun adjust_voting_power(
    info_list: &mut vector<VotingPowerInfoV2>,
    threshold: u64,
    mut remaining_power: u64,
) {
    let mut i = 0;
    let len = info_list.length();
    while (i < len && remaining_power > 0) {
        let v = &mut info_list[i];
        // planned is the amount of extra power we want to distribute to this validator.
        let planned = remaining_power.divide_and_round_up(len - i);
        // target is the targeting power this validator will reach, capped by threshold.
        let target = threshold.min(v.voting_power + planned);
        // actual is the actual amount of power we will be distributing to this validator.
        let actual = remaining_power.min(target - v.voting_power);
        v.voting_power = v.voting_power + actual;
        assert!(v.voting_power <= threshold, EVotingPowerOverThreshold);
        remaining_power = remaining_power - actual;
        i = i + 1;
    };
    assert!(remaining_power == 0, ETotalPowerMismatch);
}

/// Update validators with the decided voting power.
fun update_voting_power(validators: &mut vector<Validator>, info_list: vector<VotingPowerInfoV2>) {
    info_list.destroy!(|VotingPowerInfoV2 { validator_index, voting_power, .. }| {
        validators[validator_index].set_voting_power(voting_power);
    });
}

/// Check a few invariants that must hold after setting the voting power.
fun check_invariants(v: &vector<Validator>) {
    let mut total = 0;
    v.do_ref!(|v| {
        let voting_power = v.voting_power();
        assert!(voting_power > 0, EInvalidVotingPower);
        total = total + voting_power;
    });

    assert!(total == TOTAL_VOTING_POWER, ETotalPowerMismatch);

    // Second check that if validator A's stake is larger than B's stake, A's
    // voting power must be no less than B's voting power; similarly, if A's
    // stake is less than B's stake, A's voting power must be no larger than
    // B's voting power.
    let length = v.length();
    length.do!(|a| {
        (a + 1).range_do!(length, |b| {
            let validator_a = &v[a];
            let validator_b = &v[b];
            let stake_a = validator_a.total_stake();
            let stake_b = validator_b.total_stake();
            let power_a = validator_a.voting_power();
            let power_b = validator_b.voting_power();
            if (stake_a > stake_b) {
                assert!(power_a >= power_b, ERelativePowerMismatch);
            };
            if (stake_a < stake_b) {
                assert!(power_a <= power_b, ERelativePowerMismatch);
            };
        })
    });
}

/// Return the (constant) total voting power
public fun total_voting_power(): u64 {
    TOTAL_VOTING_POWER
}

/// Return the (constant) quorum threshold
public fun quorum_threshold(): u64 {
    QUORUM_THRESHOLD
}
