// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::forwarding_address_tests;

use sui::forwarding_address::{Self, ForwardingAddressRegistry, MasterCap};
use sui::test_scenario;

const ALICE: address = @0xA11CE;
const BOB: address = @0xB0B;
const RESERVED_MASTER_ID: u32 = 0;
const LAST_COUNTER: u64 = 0xFFFF_FFFF;

#[test]
fun register_allocates_distinct_ids_owned_by_the_registrant() {
    let mut scenario = test_scenario::begin(ALICE);
    forwarding_address::share_for_testing(scenario.ctx());

    scenario.next_tx(ALICE);
    let mut registry = scenario.take_shared<ForwardingAddressRegistry>();
    let alice_cap = forwarding_address::register(&mut registry, scenario.ctx());
    let alice_id = alice_cap.master_id();
    transfer::public_transfer(alice_cap, ALICE);
    test_scenario::return_shared(registry);

    scenario.next_tx(BOB);
    let mut registry = scenario.take_shared<ForwardingAddressRegistry>();
    let bob_cap = forwarding_address::register(&mut registry, scenario.ctx());
    let bob_id = bob_cap.master_id();
    transfer::public_transfer(bob_cap, BOB);

    assert!(alice_id != RESERVED_MASTER_ID);
    assert!(bob_id != RESERVED_MASTER_ID);
    assert!(alice_id != bob_id);
    assert!(forwarding_address::registered_master_for_testing(&registry, alice_id) == option::some(ALICE));
    assert!(forwarding_address::registered_master_for_testing(&registry, bob_id) == option::some(BOB));
    assert!(forwarding_address::registered_master_for_testing(&registry, RESERVED_MASTER_ID).is_none());
    test_scenario::return_shared(registry);

    scenario.next_tx(ALICE);
    let alice_cap = scenario.take_from_sender<MasterCap>();
    assert!(alice_cap.master_id() == alice_id);
    scenario.return_to_sender(alice_cap);

    scenario.next_tx(BOB);
    let bob_cap = scenario.take_from_sender<MasterCap>();
    assert!(bob_cap.master_id() == bob_id);
    scenario.return_to_sender(bob_cap);
    scenario.end();
}

#[test]
fun the_last_counter_value_is_allocatable() {
    let mut scenario = test_scenario::begin(ALICE);
    forwarding_address::share_for_testing(scenario.ctx());

    scenario.next_tx(ALICE);
    let mut registry = scenario.take_shared<ForwardingAddressRegistry>();
    forwarding_address::set_next_counter_for_testing(&mut registry, LAST_COUNTER);
    let cap = forwarding_address::register(&mut registry, scenario.ctx());
    assert!(cap.master_id() == forwarding_address::mix_master_id_for_testing(0xFFFF_FFFF));
    transfer::public_transfer(cap, ALICE);
    test_scenario::return_shared(registry);
    scenario.end();
}

#[test, expected_failure(abort_code = sui::forwarding_address::EMasterIdsExhausted)]
fun register_aborts_once_ids_are_exhausted() {
    let mut scenario = test_scenario::begin(ALICE);
    forwarding_address::share_for_testing(scenario.ctx());

    scenario.next_tx(ALICE);
    let mut registry = scenario.take_shared<ForwardingAddressRegistry>();
    forwarding_address::set_next_counter_for_testing(&mut registry, LAST_COUNTER + 1);
    let cap = forwarding_address::register(&mut registry, scenario.ctx());
    transfer::public_transfer(cap, ALICE);
    test_scenario::return_shared(registry);
    scenario.end();
}

#[test]
fun master_id_mixing_is_invertible_and_keeps_zero_reserved() {
    assert!(forwarding_address::mix_master_id_for_testing(0) == RESERVED_MASTER_ID);
    let samples = vector[1u32, 2, 3, 0x1234_5678, 0xDEAD_BEEF, 0xFFFF_FFFE, 0xFFFF_FFFF];
    let mut i = 0;
    while (i < samples.length()) {
        let x = samples[i];
        let mixed = forwarding_address::mix_master_id_for_testing(x);
        assert!(mixed != RESERVED_MASTER_ID);
        assert!(forwarding_address::unmix_master_id_for_testing(mixed) == x);
        i = i + 1;
    };
    assert!(forwarding_address::mix_master_id_for_testing(1) == 0x688990c0);
    assert!(forwarding_address::mix_master_id_for_testing(2) == 0xd1132181);
}
