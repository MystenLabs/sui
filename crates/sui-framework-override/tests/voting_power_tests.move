// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::voting_power_tests {
    use sui::governance_test_utils as gtu;
    use sui::validator_set;
    use sui::voting_power;
    use sui::test_utils;
    use sui::tx_context;
    use std::vector;
    use sui::validator::Validator;
    use sui::validator;

    const TOTAL_VOTING_POWER: u64 = 10_000;
    const MAX_VOTING_POWER: u64 = 1_000;

    fun check(stakes: vector<u64>, voting_power: vector<u64>) {
        let ctx = tx_context::dummy();
        let validators = gtu::create_validators_with_stakes(stakes, &mut ctx);
        voting_power::set_voting_power(&mut validators);
        test_utils::assert_eq(get_voting_power(&validators), voting_power);
        validator_set::destroy_validators_for_testing(validators)
    }

    #[test]
    fun test_small_validator_sets() {
        check(vector[1], vector[TOTAL_VOTING_POWER]);
        check(vector[77], vector[TOTAL_VOTING_POWER]);
        check(vector[TOTAL_VOTING_POWER * 93], vector[TOTAL_VOTING_POWER]);
        check(vector[1, 1], vector[5_000, 5_000]);
        check(vector[1, 2], vector[5_000, 5_000]);
        check(vector[1, 1, 1], vector[3_333, 3_333, 3_334]);
        check(vector[1, 1, 2], vector[3_333, 3_333, 3_334]);
        check(vector[1, 1, 1, 1], vector[2_500, 2_500, 2_500, 2_500]);
        check(vector[1, 1, 1, 1, 1, 1], vector[1666, 1666, 1667, 1667, 1667, 1667]);
        check(vector[1, 1, 1, 1, 1, 1, 1], vector[1428, 1428, 1428, 1429, 1429, 1429, 1429]);
        check(vector[1, 1, 1, 1, 1, 1, 1, 1, 1], vector[1111, 1111, 1111, 1111, 1111, 1111, 1111, 1111, 1112]);
        // different stake distributions that all lead to 10 validators, all with max voting power
        check(vector[1, 1, 1, 1, 1, 1, 1, 1, 1, 1], vector[1_000, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000]);
        check(vector[2, 1, 1, 1, 1, 1, 1, 1, 1, 1], vector[1_000, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000]);
        check(vector[1, 2, 3, 4, 5, 6, 7, 8, 9, 10], vector[1_000, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000]);
    }

    #[test]
    fun test_medium_validator_sets() {
        // >10 validators. now things get a bit more interesting because we can redistribute stake away from the max validators
        check(vector[1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1], vector[909, 909, 909, 909, 909, 909, 909, 909, 909, 909, 910]);
        check(vector[2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1], vector[1000, 900, 900, 900, 900, 900, 900, 900, 900, 900, 900]);
        check(vector[2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1], vector[1000, 1000, 888, 889, 889, 889, 889, 889, 889, 889, 889]);
        check(vector[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11], vector[522, 674, 826, 978, 1000, 1000, 1000, 1000, 1000, 1000, 1000]);

        // more validators, harder to reach max
        check(vector[2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1], vector[953, 953, 476, 476, 476, 476, 476, 476, 476, 476, 476, 476, 476, 476, 476, 476, 476, 477, 477]);
        check(vector[4, 3, 3, 3, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1], vector[1000, 951, 951, 951, 639, 639, 639, 325, 325, 325, 325, 325, 325, 325, 325, 326, 326, 326, 326, 326]);
    }

    fun get_voting_power(validators: &vector<Validator>): vector<u64> {
        let result = vector[];
        let i = 0;
        let len = vector::length(validators);
        while (i < len) {
            let voting_power = validator::voting_power(vector::borrow(validators, i));
            vector::push_back(&mut result, voting_power);
            i = i + 1;
        };
        result
    }
}
