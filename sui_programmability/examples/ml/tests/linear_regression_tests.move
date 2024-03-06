// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module ml::linear_regression_tests {
    use sui::test_scenario::{Self};
    use ml::linear_regression::{Self, Model};

    #[test]
    fun test_regression() {
        let user1 = @0x0;
        let user2 = @0x1;

        let scenario_val = test_scenario::begin(user1);
        let scenario = &mut scenario_val;

        linear_regression::create(test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, user1);
        let model_val = test_scenario::take_shared<Model>(scenario);
        let model = &mut model_val;

        // User1 submits a point
        test_scenario::next_tx(scenario, user1);
        linear_regression::submit_point(model, 2 << 32, false, 4 << 32, false);

        // User2 submits a point
        test_scenario::next_tx(scenario, user2);
        linear_regression::submit_point(model, 3 << 32, false, 5 << 32, false);

        test_scenario::return_shared(model_val);
        test_scenario::end(scenario_val);
    }
}
