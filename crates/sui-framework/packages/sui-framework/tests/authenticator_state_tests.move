
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::authenticator_state_tests {
    use sui::test_scenario::{Self, Scenario};
    use sui::authenticator_state::{Self, AuthenticatorState};
    use sui::tx_context;


    #[test]
    fun creating_a_clock_and_incrementing_it() {
        let scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;

        authenticator_state::create(test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, @0x0);

        let auth_state = test_scenario::take_shared<AuthenticatorState>(scenario);
        test_scenario::return_shared(auth_state);

        test_scenario::end(scenario_val);
    }
}
