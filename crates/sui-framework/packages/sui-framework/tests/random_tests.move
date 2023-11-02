
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
#[allow(unused_use)]
module sui::random_tests {
    use std::vector;

    use sui::test_scenario::{Self};
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
            1,
            vector[0, 1, 2, 3],
            test_scenario::ctx(scenario)
        );

        // TODO: Add tests once user-facing 

        test_scenario::return_shared(random_state);
        test_scenario::end(scenario_val);
    }
}
