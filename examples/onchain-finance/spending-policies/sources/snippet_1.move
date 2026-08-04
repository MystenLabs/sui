// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module example::spending_mandate_tests;

use example::spending_mandate::{create_mandate, execute_spend, remaining_cap, SpendingMandate};
use sui::balance;
use sui::clock;
use sui::sui::SUI;
use sui::test_scenario;

// docs::#mandate-tests
#[test]
fun test_spend_within_limits() {
    let owner = @0xA;
    let agent = @0xB;
    let recipient = @0xC;
    let mut scenario = test_scenario::begin(owner);
    let funds = balance::create_for_testing<SUI>(100_000_000_000);
    let owner_cap = create_mandate(
        funds,
        agent,
        10_000_000_000,
        vector[recipient],
        1_000_000_000_000,
        scenario.ctx(),
    );

    scenario.next_tx(agent);
    let mut mandate = scenario.take_from_sender<SpendingMandate<SUI>>();
    let clock = clock::create_for_testing(scenario.ctx());
    execute_spend(&mut mandate, 5_000_000_000, recipient, &clock, scenario.ctx());
    assert!(remaining_cap(&mandate) == 95_000_000_000);

    test_scenario::return_to_sender(&scenario, mandate);
    clock.destroy_for_testing();
    transfer::public_transfer(owner_cap, owner);
    scenario.end();
}

#[test]
#[expected_failure(abort_code = example::spending_mandate::EExceedsPerTxLimit)]
fun test_spend_exceeds_per_tx_limit() {
    let owner = @0xA;
    let agent = @0xB;
    let recipient = @0xC;
    let mut scenario = test_scenario::begin(owner);
    let funds = balance::create_for_testing<SUI>(100_000_000_000);
    let owner_cap = create_mandate(
        funds,
        agent,
        10_000_000_000,
        vector[recipient],
        1_000_000_000_000,
        scenario.ctx(),
    );

    scenario.next_tx(agent);
    let mut mandate = scenario.take_from_sender<SpendingMandate<SUI>>();
    let clock = clock::create_for_testing(scenario.ctx());
    execute_spend(&mut mandate, 20_000_000_000, recipient, &clock, scenario.ctx());

    test_scenario::return_to_sender(&scenario, mandate);
    clock.destroy_for_testing();
    transfer::public_transfer(owner_cap, owner);
    scenario.end();
}
// docs::/#mandate-tests
