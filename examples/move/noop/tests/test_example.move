// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module noop::example_test {
    use sui::test_scenario::{Self, Scenario};
    use noop::example::{Self, Metadata};
    use sui::transfer::{Self};
    use sui::clock::{Self};

    // Test address
    const USER: address = @0xCAFE;

    fun add_metadata(scenario: &mut Scenario) {
        test_scenario::next_tx(scenario, USER);
        {
            let ctx = test_scenario::ctx(scenario);
            let clock = clock::create_for_testing(ctx);
            
            let metadata = example::add_metadata(vector[1, 2], &clock, ctx);

            transfer::public_transfer(metadata, USER);
            clock::destroy_for_testing(clock);
        };
    }

    #[test]
    fun noop() {
        let scenario_val = test_scenario::begin(USER);
        let scenario = &mut scenario_val;
        

        test_scenario::next_tx(scenario, USER);
        {
            example::noop();
        };

        test_scenario::end(scenario_val);
    }

    #[test]
    fun noop_w_metadata() {
        let scenario_val = test_scenario::begin(USER);
        let scenario = &mut scenario_val;
        

        test_scenario::next_tx(scenario, USER);
        {
            example::noop_w_metadata(vector[1, 2]);
        };

        test_scenario::end(scenario_val);
    }

    #[test]
    fun noop_w_metadata_event() {
        let scenario_val = test_scenario::begin(USER);
        let scenario = &mut scenario_val;
        

        test_scenario::next_tx(scenario, USER);
        {
            let ctx = test_scenario::ctx(scenario);

            example::noop_w_metadata_event(vector[1, 2], ctx);
        };

        test_scenario::end(scenario_val);
    }

    #[test]
    fun adds_metadata() {
        let scenario_val = test_scenario::begin(USER);
        let scenario = &mut scenario_val;
        add_metadata(scenario);

        test_scenario::end(scenario_val);
    }

    #[test]
    fun time_since_last_heartbeat() {
        let scenario_val = test_scenario::begin(USER);
        let scenario = &mut scenario_val;
        add_metadata(scenario);
        
        test_scenario::next_tx(scenario, USER);
        {
            let ctx = test_scenario::ctx(scenario);
            let clock = clock::create_for_testing(ctx);
            let metadata = test_scenario::take_from_sender<Metadata>(scenario);
            
            example::time_since_last_heartbeat(&metadata, &clock);

            clock::destroy_for_testing(clock);
            test_scenario::return_to_sender(scenario, metadata);
        };

        test_scenario::end(scenario_val);
    }
}
