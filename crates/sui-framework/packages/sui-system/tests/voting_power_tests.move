// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui_system::voting_power_tests;

use std::unit_test::{assert_eq, destroy};
use sui_system::validator::{Self, Validator};
use sui_system::validator_builder;
use sui_system::validator_set;
use sui_system::voting_power;

const TOTAL_VOTING_POWER: u64 = 10_000;

fun check(stakes: vector<u64>, voting_power: vector<u64>, ctx: &mut TxContext) {
    let mut validators = create_validators_with_stakes(stakes, ctx);
    let total_stake = validator_set::calculate_total_stakes(&validators);
    voting_power::set_voting_power(&mut validators, total_stake);

    // get the voting powers of the validators
    let voting_powers = vector::tabulate!(
        validators.length(),
        |i| validator::voting_power(&validators[i]),
    );

    assert_eq!(voting_powers, voting_power);
    destroy(validators);
}

#[test]
fun small_validator_sets() {
    let ctx = &mut tx_context::dummy();

    check(vector[1], vector[TOTAL_VOTING_POWER], ctx);
    check(vector[77], vector[TOTAL_VOTING_POWER], ctx);
    check(vector[TOTAL_VOTING_POWER * 93], vector[TOTAL_VOTING_POWER], ctx);
    check(vector[1, 1], vector[5_000, 5_000], ctx);
    check(vector[1, 2], vector[5_000, 5_000], ctx);
    check(vector[1, 1, 1], vector[3_333, 3_333, 3_334], ctx);
    check(vector[1, 1, 2], vector[3_333, 3_333, 3_334], ctx);
    check(vector[1, 1, 1, 1], vector[2_500, 2_500, 2_500, 2_500], ctx);
    check(vector[1, 1, 1, 1, 1, 1], vector[1666, 1666, 1667, 1667, 1667, 1667], ctx);
    check(vector[1, 1, 1, 1, 1, 1, 1], vector[1428, 1428, 1428, 1429, 1429, 1429, 1429], ctx);
    check(
        vector[1, 1, 1, 1, 1, 1, 1, 1, 1],
        vector[1111, 1111, 1111, 1111, 1111, 1111, 1111, 1111, 1112],
        ctx,
    );
    // different stake distributions that all lead to 10 validators, all with max voting power
    check(
        vector[1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
        vector[1_000, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000],
        ctx,
    );
    check(
        vector[2, 1, 1, 1, 1, 1, 1, 1, 1, 1],
        vector[1_000, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000],
        ctx,
    );
    check(
        vector[1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
        vector[1_000, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000],
        ctx,
    );
    // This tests the scenario where we have validators whose stakes are only slightly different.
    // Make sure that the order is preserved correctly and the leftover voting power goes to the right validators.
    check(vector[10000, 10001, 10000], vector[3333, 3334, 3333], ctx);
}

#[test]
fun medium_validator_sets() {
    let ctx = &mut tx_context::dummy();
    // >10 validators. now things get a bit more interesting because we can redistribute stake away from the max validators
    check(
        vector[1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
        vector[909, 909, 909, 909, 909, 909, 909, 909, 909, 909, 910],
        ctx,
    );
    check(
        vector[2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
        vector[1000, 900, 900, 900, 900, 900, 900, 900, 900, 900, 900],
        ctx,
    );
    check(
        vector[2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1],
        vector[1000, 1000, 888, 889, 889, 889, 889, 889, 889, 889, 889],
        ctx,
    );
    check(
        vector[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
        vector[522, 674, 826, 978, 1000, 1000, 1000, 1000, 1000, 1000, 1000],
        ctx,
    );
}

#[test]
fun medium_validator_sets_2() {
    let ctx = &mut tx_context::dummy();

    // more validators, harder to reach max
    // prettier-ignore
    check(
        vector[2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
        vector[953, 953, 476, 476, 476, 476, 476, 476, 476, 476, 476, 476, 476, 476, 476, 476, 476, 477, 477],
        ctx,
    );

    // prettier-ignore
    check(
        vector[4, 3, 3, 3, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
        vector[1000, 951, 951, 951, 639, 639, 639, 325, 325, 325, 325, 325, 325, 325, 325, 326, 326, 326, 326, 326],
        ctx,
    );
}

/// Create a validator set with the given stake amounts
fun create_validators_with_stakes(stakes: vector<u64>, ctx: &mut TxContext): vector<Validator> {
    vector::tabulate!(stakes.length(), |i| {
        validator_builder::new()
            .initial_stake(stakes[i])
            .sui_address(sui::address::from_u256(i as u256))
            .build(ctx)
    })
}
