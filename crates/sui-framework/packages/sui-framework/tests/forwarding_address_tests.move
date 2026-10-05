// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::forwarding_address_tests;

use sui::balance;
use sui::forwarding_address::{Self, ForwardingAddressRegistry, MasterCap};
use sui::sui::SUI;
use sui::test_scenario::{Self, Scenario};

const ALICE: address = @0xA11CE;
const BOB: address = @0xB0B;
const RESERVED_MASTER_ID: u32 = 0;
const LAST_COUNTER: u64 = 0xFFFF_FFFF;
const FIRST_MASTER_ID: u32 = 0x688990c0;
const OPAQUE_VARIANT: u8 = 0;

#[test]
fun register_allocates_distinct_ids_owned_by_the_registrant() {
    let mut scenario = test_scenario::begin(@0x0);
    forwarding_address::create_for_testing(scenario.ctx());

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
    assert!(
        forwarding_address::registered_master_for_testing(&registry, alice_id) == option::some(ALICE),
    );
    assert!(
        forwarding_address::registered_master_for_testing(&registry, bob_id) == option::some(BOB),
    );
    assert!(
        forwarding_address::registered_master_for_testing(&registry, RESERVED_MASTER_ID).is_none(),
    );
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
    let mut scenario = test_scenario::begin(@0x0);
    forwarding_address::create_for_testing(scenario.ctx());

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
    let mut scenario = test_scenario::begin(@0x0);
    forwarding_address::create_for_testing(scenario.ctx());

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

/// `[u32 master_id LE][0xfa x 10][u8 variant][payload_byte x 17]`.
fun forwarding_address(master_id: u32, variant: u8, payload_byte: u8): address {
    let mut bytes = vector[];
    let mut i = 0;
    while (i < 4) {
        bytes.push_back(((master_id >> (8 * i)) & 0xff) as u8);
        i = i + 1;
    };
    10u64.do!(|_| bytes.push_back(0xfa));
    bytes.push_back(variant);
    17u64.do!(|_| bytes.push_back(payload_byte));
    sui::address::from_bytes(bytes)
}

/// Shares the registry, registers `master`, and starts a transaction from `depositor`.
fun register_then_switch_to(master: address, depositor: address): (Scenario, u32) {
    let mut scenario = test_scenario::begin(@0x0);
    forwarding_address::create_for_testing(scenario.ctx());
    scenario.next_tx(master);
    let mut registry = scenario.take_shared<ForwardingAddressRegistry>();
    let cap = forwarding_address::register(&mut registry, scenario.ctx());
    let master_id = cap.master_id();
    transfer::public_transfer(cap, master);
    test_scenario::return_shared(registry);
    scenario.next_tx(depositor);
    (scenario, master_id)
}

#[test]
fun deposits_to_forwarding_addresses_credit_the_master() {
    let (mut scenario, master_id) = register_then_switch_to(ALICE, BOB);
    assert!(master_id == FIRST_MASTER_ID);
    let first = forwarding_address(master_id, OPAQUE_VARIANT, 1);
    let second = forwarding_address(master_id, OPAQUE_VARIANT, 2);
    balance::create_for_testing<SUI>(1000).send_funds(first);
    sui::coin::mint_for_testing<SUI>(500, scenario.ctx()).send_funds(second);

    scenario.next_tx(BOB);
    assert!(test_scenario::settled_balance<SUI>(ALICE) == 1500);
    assert!(test_scenario::settled_balance<SUI>(first) == 0);
    assert!(test_scenario::settled_balance<SUI>(second) == 0);
    scenario.end();
}

#[test]
fun magic_at_the_wrong_offset_is_an_ordinary_address() {
    let (mut scenario, master_id) = register_then_switch_to(ALICE, BOB);
    let mut bytes = forwarding_address(master_id, OPAQUE_VARIANT, 1).to_bytes();
    *&mut bytes[4] = 0;
    let ordinary = sui::address::from_bytes(bytes);
    balance::create_for_testing<SUI>(1000).send_funds(ordinary);

    scenario.next_tx(BOB);
    assert!(test_scenario::settled_balance<SUI>(ordinary) == 1000);
    assert!(test_scenario::settled_balance<SUI>(ALICE) == 0);
    scenario.end();
}

#[test, expected_failure(abort_code = 1, location = sui::forwarding_address)]
fun deposit_to_an_unregistered_id_aborts() {
    let (mut scenario, _) = register_then_switch_to(ALICE, BOB);
    let unregistered = forwarding_address::mix_master_id_for_testing(2);
    balance::create_for_testing<SUI>(1000).send_funds(
        forwarding_address(unregistered, OPAQUE_VARIANT, 1),
    );
    scenario.next_tx(BOB);
    scenario.end();
}

#[test, expected_failure(abort_code = 2, location = sui::forwarding_address)]
fun deposit_with_an_unsupported_variant_aborts() {
    let (mut scenario, master_id) = register_then_switch_to(ALICE, BOB);
    balance::create_for_testing<SUI>(1000).send_funds(forwarding_address(master_id, 1, 1));
    scenario.next_tx(BOB);
    scenario.end();
}

// Resolution reads the registry as committed before the transaction, like a deposit on chain.
#[test, expected_failure(abort_code = 1, location = sui::forwarding_address)]
fun deposit_to_an_id_registered_in_the_same_transaction_aborts() {
    let mut scenario = test_scenario::begin(@0x0);
    forwarding_address::create_for_testing(scenario.ctx());
    scenario.next_tx(ALICE);
    let mut registry = scenario.take_shared<ForwardingAddressRegistry>();
    let cap = forwarding_address::register(&mut registry, scenario.ctx());
    balance::create_for_testing<SUI>(1000).send_funds(
        forwarding_address(cap.master_id(), OPAQUE_VARIANT, 1),
    );
    transfer::public_transfer(cap, ALICE);
    test_scenario::return_shared(registry);
    scenario.end();
}
