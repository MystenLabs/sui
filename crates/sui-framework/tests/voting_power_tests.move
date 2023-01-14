// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::voting_power_tests {
    use sui::governance_test_utils as gtu;
    use sui::validator_set;
    use sui::voting_power;
    use sui::test;
    use sui::tx_context;

    const TOTAL_VOTING_POWER: u64 = 10_000;
    const MAX_VOTING_POWER: u64 = 1_000;

    fun check(stakes: vector<u64>, voting_power: vector<u64>) {
        let ctx = tx_context::dummy();
        let validators = gtu::create_validators_with_stakes(stakes, &mut ctx);
        voting_power::update(&mut validators);
        test::assert_eq(voting_power::voting_power(&validators), voting_power);
        validator_set::destroy_validators_for_testing(validators)
    }

    #[test]
    fun test_small_validator_sets() {
        check(vector[1], vector[TOTAL_VOTING_POWER]);
        check(vector[77], vector[TOTAL_VOTING_POWER]);
        check(vector[TOTAL_VOTING_POWER * 93], vector[TOTAL_VOTING_POWER]);
        check(vector[1, 1], vector[5_000, 5_000]);
        check(vector[1, 2], vector[5_000, 5_000]);
        check(vector[1, 1, 1], vector[3_334, 3_333, 3_333]);
        check(vector[1, 1, 2], vector[3_334, 3_333, 3_333]);
        check(vector[1, 1, 1, 1], vector[2_500, 2_500, 2_500, 2_500]);
        check(vector[1, 1, 1, 1, 1, 1], vector[1667, 1667, 1667, 1667, 1666, 1666]);
        check(vector[1, 1, 1, 1, 1, 1, 1], vector[1429, 1429, 1429, 1429, 1428, 1428, 1428]);
        check(vector[1, 1, 1, 1, 1, 1, 1, 1, 1], vector[1112, 1111, 1111, 1111, 1111, 1111, 1111, 1111, 1111]);
        // different stake distributions that all lead to 10 validators, all with max voting power
        check(vector[1, 1, 1, 1, 1, 1, 1, 1, 1, 1], vector[1_000, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000]);
        check(vector[2, 1, 1, 1, 1, 1, 1, 1, 1, 1], vector[1_000, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000]);
        check(vector[1, 2, 3, 4, 5, 6, 7, 8, 9, 10], vector[1_000, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000]);
    }

    #[test]
    fun test_medium_validator_sets() {
        // >10 validators. now things get a bit more interesting because we can redistribute stake away from the max validators
        check(vector[1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1], vector[910, 909, 909, 909, 909, 909, 909, 909, 909, 909, 909]);
        check(vector[2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1], vector[1000, 900, 900, 900, 900, 900, 900, 900, 900, 900, 900]);
        check(vector[2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1], vector[1000, 1000, 889, 889, 889, 889, 889, 889, 889, 889, 888]);
        check(vector[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11], vector[1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000, 672, 328]);

        // more validators, harder to reach max
        check(vector[2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1], vector[953, 953, 477, 477, 476, 476, 476, 476, 476, 476, 476, 476, 476, 476, 476, 476, 476, 476, 476]);
        check(vector[4, 3, 3, 3, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1], vector[1000, 965, 965, 965, 644, 644, 644, 321, 321, 321, 321, 321, 321, 321, 321, 321, 321, 321, 321, 321]);
    }
}