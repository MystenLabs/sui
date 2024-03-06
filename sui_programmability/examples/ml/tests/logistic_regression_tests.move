// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module ml::logistic_regression_tests {
    use sui::test_scenario::{Self};
    use ml::logistic_regression::{Self, Model, evaluate};
    use ml::ifixed_point32::{from_rational, zero, one, from_raw};

    #[test]
    fun test_prediction() {
        let b = vector[from_rational(1, 10, false), from_rational(2, 10, false), from_rational(3, 10, false)];
        let x = vector[from_rational(3, 2, false), from_rational(5, 2, false)];
        let eval = evaluate(&b, &x);
        let expected = from_raw(3262074553, false);
        assert!(eval == expected, 0);
    }

    #[test]
    fun test_regression_1() {
        let user1 = @0x0;

        let scenario_val = test_scenario::begin(user1);
        let scenario = &mut scenario_val;

        logistic_regression::create(3, test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, user1);
        let model_val = test_scenario::take_shared<Model>(scenario);
        let model = &mut model_val;

        // User1 submits a point
        test_scenario::next_tx(scenario, user1);

        let data = vector[
            vector[from_rational(3, 2, false), from_rational(5, 2, false)],
            vector[from_rational(21, 10, false), from_rational(31, 10, false)],
            vector[from_rational(16, 5, false), from_rational(21, 5, false)]
        ];

        let expected = vector[zero(), zero(), one()];

        logistic_regression::fit(model, &data, &expected, from_rational(1, 10, false), 5);

        std::debug::print(model);

        test_scenario::return_shared(model_val);
        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_regression_2() {
        let user1 = @0x0;

        let scenario_val = test_scenario::begin(user1);
        let scenario = &mut scenario_val;

        logistic_regression::create(2, test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, user1);
        let model_val = test_scenario::take_shared<Model>(scenario);
        let model = &mut model_val;

        // User1 submits a point
        test_scenario::next_tx(scenario, user1);

        let data = vector[
            vector[from_rational(1, 2, false)],
            vector[from_rational(3, 4, false)],
            vector[from_rational(1, 1, false)],
            vector[from_rational(5, 4, false)],
            vector[from_rational(3, 2, false)],
            vector[from_rational(7, 4, false)],
            vector[from_rational(7, 4, false)],
            vector[from_rational(2, 1, false)],
            vector[from_rational(9, 4, false)],
            vector[from_rational(5, 2, false)],
            vector[from_rational(11, 4, false)],
            vector[from_rational(3, 1, false)],
            vector[from_rational(13, 4, false)],
            vector[from_rational(7, 2, false)],
            vector[from_rational(4, 1, false)],
            vector[from_rational(17, 4, false)],
            vector[from_rational(9, 2, false)],
            vector[from_rational(19, 4, false)],
            vector[from_rational(5, 1, false)],
            vector[from_rational(11, 2, false)]
        ];
        let expected = vector[
            zero(), zero(), zero(), zero(), 
            zero(), zero(), one(), zero(), 
            one(), zero(), one(), zero(), 
            one(), zero(), one(), one(), 
            one(), one(), one(), one()];
        logistic_regression::fit(model, &data, &expected, from_rational(1, 1, false), 40);
        std::debug::print(model);

        test_scenario::return_shared(model_val);
        test_scenario::end(scenario_val);
    }
}
