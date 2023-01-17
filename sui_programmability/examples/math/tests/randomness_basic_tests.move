// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module math::randomness_basic_tests {
    use std::option;
    use sui::randomness;
    use sui::test_scenario;
    use math::randomness_basics;
    use sui::randomness::Randomness;

    const TEST_USER_ADDR: address = @0xA11CE;

    #[test]
    fun test_owned_object() {
        let scenario_val = test_scenario::begin(TEST_USER_ADDR);
        let scenario = &mut scenario_val;
        let ctx = test_scenario::ctx(scenario);

        randomness_basics::create_owned_randomness(ctx);
        test_scenario::next_tx(scenario, TEST_USER_ADDR);
        let r = test_scenario::take_from_sender<Randomness<randomness_basics::WITNESS>>(scenario);
        assert!(option::is_none(randomness::value(&r)), 0);

        // Get a valid signature and set the object.
        let sig = randomness::sign(&r);
        randomness_basics::set_randomness(&mut r, sig);
        assert!(option::is_some(randomness::value(&r)), 0);

        randomness::destroy(r);
        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_shared_object() {
        let scenario_val = test_scenario::begin(TEST_USER_ADDR);
        let scenario = &mut scenario_val;
        let ctx = test_scenario::ctx(scenario);

        randomness_basics::create_shared_randomness(ctx);
        test_scenario::next_tx(scenario, TEST_USER_ADDR);
        let r = test_scenario::take_shared<Randomness<randomness_basics::WITNESS>>(scenario);
        assert!(option::is_none(randomness::value(&r)), 0);

        // Get a valid signature and set the object.
        let sig = randomness::sign(&r);
        randomness_basics::set_randomness(&mut r, sig);
        assert!(option::is_some(randomness::value(&r)), 0);

        test_scenario::return_shared(r);
        test_scenario::end(scenario_val);
    }
}
