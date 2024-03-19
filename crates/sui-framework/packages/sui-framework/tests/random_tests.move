
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
#[allow(unused_use)]
module sui::random_tests {
    use std::vector;

    use sui::test_scenario;
    use sui::random::{
        Self,
        Random,
        update_randomness_state_for_testing,
    };

    #[test]
    fun random_tests_basic() {
        let scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;

        random::create_for_testing(test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, @0x0);

        let random_state = test_scenario::take_shared<Random>(scenario);
        update_randomness_state_for_testing(
            &mut random_state,
            0,
            vector[0, 1, 2, 3],
            test_scenario::ctx(scenario)
        );

        // TODO: Add more once user-facing API is implemented.

        test_scenario::return_shared(random_state);
        test_scenario::end(scenario_val);
    }

    #[test]
    fun random_tests_update_after_epoch_change() {
        let scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;

        random::create_for_testing(test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, @0x0);

        let random_state = test_scenario::take_shared<Random>(scenario);
        update_randomness_state_for_testing(
            &mut random_state,
            0,
            vector[0, 1, 2, 3],
            test_scenario::ctx(scenario)
        );
        update_randomness_state_for_testing(
            &mut random_state,
            1,
            vector[4, 5, 6, 7],
            test_scenario::ctx(scenario)
        );

        test_scenario::next_epoch(scenario, @0x0);

        update_randomness_state_for_testing(
            &mut random_state,
            0,
            vector[8, 9, 10, 11],
            test_scenario::ctx(scenario)
        );

        test_scenario::return_shared(random_state);
        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = random::EInvalidRandomnessUpdate)]
    fun random_tests_duplicate() {
        let scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;

        random::create_for_testing(test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, @0x0);

        let random_state = test_scenario::take_shared<Random>(scenario);
        update_randomness_state_for_testing(
            &mut random_state,
            0,
            vector[0, 1, 2, 3],
            test_scenario::ctx(scenario)
        );
        update_randomness_state_for_testing(
            &mut random_state,
            0,
            vector[0, 1, 2, 3],
            test_scenario::ctx(scenario)
        );

        test_scenario::return_shared(random_state);
        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = random::EInvalidRandomnessUpdate)]
    fun random_tests_out_of_order() {
        let scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;

        random::create_for_testing(test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, @0x0);

        let random_state = test_scenario::take_shared<Random>(scenario);
        update_randomness_state_for_testing(
            &mut random_state,
            0,
            vector[0, 1, 2, 3],
            test_scenario::ctx(scenario)
        );
        update_randomness_state_for_testing(
            &mut random_state,
            3,
            vector[0, 1, 2, 3],
            test_scenario::ctx(scenario)
        );

        test_scenario::return_shared(random_state);
        test_scenario::end(scenario_val);
    }
}
