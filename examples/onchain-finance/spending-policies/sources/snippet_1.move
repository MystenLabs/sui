// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module example::spending_mandate_tests;

use example::spending_mandate::{
    Self,
    SpendingMandate,
    create_mandate,
    execute_spend,
    remaining_cap
};
use sui::clock;
use sui::coin;
use sui::sui::SUI;
use sui::test_scenario;

const EExceedsPerTxLimit: u64 = 1;

// docs::#mandate-tests
#[test]
fun test_spend_within_limits() {
    let owner = @0xA;
    let agent = @0xB;
    let recipient = @0xC;

    let mut scenario = test_scenario::begin(owner);

    // Owner creates mandate
    let owner_cap = create_mandate(
        agent,
        10_000_000_000, // max 10 SUI per tx
        100_000_000_000, // 100 SUI total cap
        vector[recipient],
        1_000_000_000_000, // far-future expiry
        scenario.ctx(),
    );

    // Agent executes a spend
    scenario.next_tx(agent);
    let mut mandate = scenario.take_from_sender<SpendingMandate>();
    let clock = clock::create_for_testing(scenario.ctx());
    let payment = coin::mint_for_testing<SUI>(5_000_000_000, scenario.ctx());

    execute_spend(&mut mandate, payment, recipient, &clock, scenario.ctx());
    assert!(remaining_cap(&mandate) == 95_000_000_000);

    // Clean up
    test_scenario::return_to_sender(&scenario, mandate);
    clock.destroy_for_testing();
    transfer::public_transfer(owner_cap, owner);
    scenario.end();
}

#[test]
#[expected_failure(abort_code = EExceedsPerTxLimit)]
fun test_spend_exceeds_per_tx_limit() {
    let owner = @0xA;
    let agent = @0xB;
    let recipient = @0xC;

    let mut scenario = test_scenario::begin(owner);

    let owner_cap = create_mandate(
        agent,
        10_000_000_000,
        100_000_000_000,
        vector[recipient],
        1_000_000_000_000,
        scenario.ctx(),
    );

    scenario.next_tx(agent);
    let mut mandate = scenario.take_from_sender<SpendingMandate>();
    let clock = clock::create_for_testing(scenario.ctx());

    // Try to spend 20 SUI (exceeds 10 SUI per-tx limit)
    let payment = coin::mint_for_testing<SUI>(20_000_000_000, scenario.ctx());
    execute_spend(&mut mandate, payment, recipient, &clock, scenario.ctx());

    // Clean up (unreachable)
    test_scenario::return_to_sender(&scenario, mandate);
    clock.destroy_for_testing();
    transfer::public_transfer(owner_cap, owner);
    scenario.end();
}
// docs::/#mandate-tests
