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
const RESERVED_MASTER_ID: u64 = 0;
const LAST_COUNTER: u64 = 0xFFFF_FFFF_FFFF;
const FIRST_MASTER_ID: u64 = 0x52ca8647179c;
const OPAQUE_VARIANT: u8 = 0;

#[test]
fun register_allocates_distinct_ids_owned_by_the_registrant() {
    let mut scenario = test_scenario::begin(@0x0);
    forwarding_address::create_for_testing(scenario.ctx());

    scenario.next_tx(ALICE);
    let mut registry = scenario.take_shared<ForwardingAddressRegistry>();
    let alice_cap = forwarding_address::register(&mut registry, 1, scenario.ctx());
    let alice_id = alice_cap.master_id();
    transfer::public_transfer(alice_cap, ALICE);
    test_scenario::return_shared(registry);

    scenario.next_tx(BOB);
    let mut registry = scenario.take_shared<ForwardingAddressRegistry>();
    let bob_cap = forwarding_address::register(&mut registry, 1, scenario.ctx());
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
    let cap = forwarding_address::register(&mut registry, 1, scenario.ctx());
    assert!(cap.master_id() == forwarding_address::mix_master_id_for_testing(0xFFFF_FFFF_FFFF));
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
    let cap = forwarding_address::register(&mut registry, 1, scenario.ctx());
    transfer::public_transfer(cap, ALICE);
    test_scenario::return_shared(registry);
    scenario.end();
}

#[test]
fun master_id_mixing_is_invertible_and_keeps_zero_reserved() {
    assert!(forwarding_address::mix_master_id_for_testing(0) == RESERVED_MASTER_ID);
    let samples = vector[
        1u64,
        2,
        3,
        0x1234_5678,
        0xDEAD_BEEF,
        0x1234_5678_9abc,
        0xFFFF_FFFF_FFFE,
        0xFFFF_FFFF_FFFF,
    ];
    let mut i = 0;
    while (i < samples.length()) {
        let x = samples[i];
        let mixed = forwarding_address::mix_master_id_for_testing(x);
        assert!(mixed != RESERVED_MASTER_ID);
        assert!(forwarding_address::unmix_master_id_for_testing(mixed) == x);
        i = i + 1;
    };
    assert!(forwarding_address::mix_master_id_for_testing(1) == 0x52ca8647179c);
    assert!(forwarding_address::mix_master_id_for_testing(2) == 0x2b339572355c);
}

/// `[u48 master_id LE][0xfa x 9][u8 variant][payload_byte x 16]`.
fun forwarding_address(master_id: u64, variant: u8, payload_byte: u8): address {
    let mut bytes = vector[];
    let mut i = 0;
    while (i < 6) {
        bytes.push_back(((master_id >> (8 * i)) & 0xff) as u8);
        i = i + 1;
    };
    9u64.do!(|_| bytes.push_back(0xfa));
    bytes.push_back(variant);
    16u64.do!(|_| bytes.push_back(payload_byte));
    sui::address::from_bytes(bytes)
}

/// Shares the registry, registers `master`, and starts a transaction from `depositor`.
fun register_then_switch_to(master: address, depositor: address): (Scenario, u64) {
    let mut scenario = test_scenario::begin(@0x0);
    forwarding_address::create_for_testing(scenario.ctx());
    scenario.next_tx(master);
    let mut registry = scenario.take_shared<ForwardingAddressRegistry>();
    let cap = forwarding_address::register(&mut registry, 1, scenario.ctx());
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
    *&mut bytes[6] = 0;
    let ordinary = sui::address::from_bytes(bytes);
    balance::create_for_testing<SUI>(1000).send_funds(ordinary);

    scenario.next_tx(BOB);
    assert!(test_scenario::settled_balance<SUI>(ordinary) == 1000);
    assert!(test_scenario::settled_balance<SUI>(ALICE) == 0);
    scenario.end();
}

#[test, expected_failure(abort_code = sui::test_scenario::EForwardingAddressUnresolvable)]
fun deposit_to_an_unregistered_id_aborts() {
    let (mut scenario, _) = register_then_switch_to(ALICE, BOB);
    let unregistered = forwarding_address::mix_master_id_for_testing(2);
    balance::create_for_testing<SUI>(1000).send_funds(
        forwarding_address(unregistered, OPAQUE_VARIANT, 1),
    );
    scenario.next_tx(BOB);
    scenario.end();
}

#[test, expected_failure(abort_code = sui::test_scenario::EForwardingAddressUnresolvable)]
fun deposit_with_an_unsupported_variant_aborts() {
    let (mut scenario, master_id) = register_then_switch_to(ALICE, BOB);
    balance::create_for_testing<SUI>(1000).send_funds(forwarding_address(master_id, 1, 1));
    scenario.next_tx(BOB);
    scenario.end();
}

// Resolution sees the registry as committed before the transaction, like a deposit on chain.
#[test, expected_failure(abort_code = sui::test_scenario::EForwardingAddressUnresolvable)]
fun deposit_to_an_id_registered_in_the_same_transaction_aborts() {
    let mut scenario = test_scenario::begin(@0x0);
    forwarding_address::create_for_testing(scenario.ctx());
    scenario.next_tx(ALICE);
    let mut registry = scenario.take_shared<ForwardingAddressRegistry>();
    let cap = forwarding_address::register(&mut registry, 1, scenario.ctx());
    balance::create_for_testing<SUI>(1000).send_funds(
        forwarding_address(cap.master_id(), OPAQUE_VARIANT, 1),
    );
    transfer::public_transfer(cap, ALICE);
    test_scenario::return_shared(registry);
    scenario.end();
}

// === Pause and rotation ===

const CAROL: address = @0xCA201;

/// Registers for ALICE and leaves the scenario in a transaction by BOB.
fun register_alice_then_switch_to_bob(): (Scenario, MasterCap) {
    let mut scenario = test_scenario::begin(@0x0);
    forwarding_address::create_for_testing(scenario.ctx());
    scenario.next_tx(ALICE);
    let mut registry = scenario.take_shared<ForwardingAddressRegistry>();
    let cap = forwarding_address::register(&mut registry, 2, scenario.ctx());
    test_scenario::return_shared(registry);
    scenario.next_tx(BOB);
    (scenario, cap)
}

#[test]
fun paused_ids_reject_deposits_until_the_cap_unpauses() {
    let (mut scenario, cap) = register_alice_then_switch_to_bob();
    let target = forwarding_address(cap.master_id(), OPAQUE_VARIANT, 1);
    let mut registry = scenario.take_shared<ForwardingAddressRegistry>();
    forwarding_address::pause(&mut registry, &cap);
    assert!(forwarding_address::is_paused_for_testing(&registry, cap.master_id()));
    // Pausing twice is a no-op, not an error.
    forwarding_address::pause(&mut registry, &cap);
    forwarding_address::unpause(&mut registry, &cap);
    assert!(!forwarding_address::is_paused_for_testing(&registry, cap.master_id()));
    test_scenario::return_shared(registry);

    scenario.next_tx(BOB);
    balance::create_for_testing<SUI>(1000).send_funds(target);
    scenario.next_tx(BOB);
    assert!(test_scenario::settled_balance<SUI>(ALICE) == 1000);
    transfer::public_transfer(cap, ALICE);
    scenario.end();
}

/// Registers from `registrant`, which may itself be a forwarding address, so that the new id's
/// master is a forwarding address and deposits to it chain. Returns the new master id.
fun register_from(scenario: &mut Scenario, registrant: address): u64 {
    scenario.next_tx(registrant);
    let mut registry = scenario.take_shared<ForwardingAddressRegistry>();
    let cap = forwarding_address::register(&mut registry, 1, scenario.ctx());
    let master_id = cap.master_id();
    transfer::public_transfer(cap, registrant);
    test_scenario::return_shared(registry);
    master_id
}

#[test]
fun deposits_follow_a_chain_of_forwarding_addresses() {
    let mut scenario = test_scenario::begin(@0x0);
    forwarding_address::create_for_testing(scenario.ctx());
    let alice_id = register_from(&mut scenario, ALICE);
    let alice_forwarding = forwarding_address(alice_id, OPAQUE_VARIANT, 1);
    // A master that is itself one of ALICE's forwarding addresses: two hops to ALICE.
    let sub_id = register_from(&mut scenario, alice_forwarding);
    let sub_forwarding = forwarding_address(sub_id, OPAQUE_VARIANT, 2);

    scenario.next_tx(BOB);
    balance::create_for_testing<SUI>(1000).send_funds(sub_forwarding);
    scenario.next_tx(BOB);
    assert!(test_scenario::settled_balance<SUI>(ALICE) == 1000);
    assert!(test_scenario::settled_balance<SUI>(alice_forwarding) == 0);
    assert!(test_scenario::settled_balance<SUI>(sub_forwarding) == 0);
    scenario.end();
}

#[test, expected_failure(abort_code = sui::test_scenario::EForwardingAddressUnresolvable)]
fun deposits_to_a_paused_id_abort() {
    let (mut scenario, cap) = register_alice_then_switch_to_bob();
    let target = forwarding_address(cap.master_id(), OPAQUE_VARIANT, 1);
    let mut registry = scenario.take_shared<ForwardingAddressRegistry>();
    forwarding_address::pause(&mut registry, &cap);
    test_scenario::return_shared(registry);
    transfer::public_transfer(cap, ALICE);
    scenario.next_tx(BOB);
    balance::create_for_testing<SUI>(1000).send_funds(target);
    scenario.next_tx(BOB);
    scenario.end();
}

#[test]
fun the_master_can_pause_without_the_cap() {
    let (mut scenario, cap) = register_alice_then_switch_to_bob();
    scenario.next_tx(ALICE);
    let mut registry = scenario.take_shared<ForwardingAddressRegistry>();
    forwarding_address::pause_by_master(&mut registry, cap.master_id(), scenario.ctx());
    assert!(forwarding_address::is_paused_for_testing(&registry, cap.master_id()));
    test_scenario::return_shared(registry);
    transfer::public_transfer(cap, ALICE);
    scenario.end();
}

#[test, expected_failure(abort_code = sui::forwarding_address::ENotMaster)]
fun only_the_master_can_pause_without_the_cap() {
    let (mut scenario, cap) = register_alice_then_switch_to_bob();
    let mut registry = scenario.take_shared<ForwardingAddressRegistry>();
    forwarding_address::pause_by_master(&mut registry, cap.master_id(), scenario.ctx());
    test_scenario::return_shared(registry);
    transfer::public_transfer(cap, ALICE);
    scenario.end();
}

#[test]
fun rotation_takes_effect_after_the_delay() {
    let (mut scenario, cap) = register_alice_then_switch_to_bob();
    let target = forwarding_address(cap.master_id(), OPAQUE_VARIANT, 1);
    let mut registry = scenario.take_shared<ForwardingAddressRegistry>();
    forwarding_address::propose_rotation(&mut registry, &cap, CAROL, scenario.ctx());
    assert!(
        forwarding_address::pending_rotation_for_testing(&registry, cap.master_id()) == option::some(CAROL),
    );
    test_scenario::return_shared(registry);

    // Pending: deposits still reach the old master.
    scenario.next_tx(BOB);
    balance::create_for_testing<SUI>(300).send_funds(target);
    scenario.next_tx(BOB);
    assert!(test_scenario::settled_balance<SUI>(ALICE) == 300);

    // One epoch later the delay of two has not elapsed; two epochs later it has, and anyone may
    // finalize.
    scenario.next_epoch(BOB);
    scenario.next_epoch(BOB);
    let mut registry = scenario.take_shared<ForwardingAddressRegistry>();
    forwarding_address::finalize_rotation(&mut registry, cap.master_id(), scenario.ctx());
    assert!(
        forwarding_address::registered_master_for_testing(&registry, cap.master_id()) == option::some(CAROL),
    );
    assert!(forwarding_address::pending_rotation_for_testing(&registry, cap.master_id()).is_none());
    test_scenario::return_shared(registry);

    scenario.next_tx(BOB);
    balance::create_for_testing<SUI>(400).send_funds(target);
    scenario.next_tx(BOB);
    assert!(test_scenario::settled_balance<SUI>(ALICE) == 300);
    assert!(test_scenario::settled_balance<SUI>(CAROL) == 400);
    transfer::public_transfer(cap, ALICE);
    scenario.end();
}

#[test, expected_failure(abort_code = sui::forwarding_address::ERotationNotDue)]
fun rotation_cannot_be_finalized_before_the_delay() {
    let (mut scenario, cap) = register_alice_then_switch_to_bob();
    let mut registry = scenario.take_shared<ForwardingAddressRegistry>();
    forwarding_address::propose_rotation(&mut registry, &cap, CAROL, scenario.ctx());
    test_scenario::return_shared(registry);
    scenario.next_epoch(BOB);
    let mut registry = scenario.take_shared<ForwardingAddressRegistry>();
    forwarding_address::finalize_rotation(&mut registry, cap.master_id(), scenario.ctx());
    test_scenario::return_shared(registry);
    transfer::public_transfer(cap, ALICE);
    scenario.end();
}

#[test]
fun the_master_can_cancel_a_rotation_without_the_cap() {
    let (mut scenario, cap) = register_alice_then_switch_to_bob();
    let mut registry = scenario.take_shared<ForwardingAddressRegistry>();
    forwarding_address::propose_rotation(&mut registry, &cap, CAROL, scenario.ctx());
    test_scenario::return_shared(registry);
    scenario.next_tx(ALICE);
    let mut registry = scenario.take_shared<ForwardingAddressRegistry>();
    forwarding_address::cancel_rotation_by_master(&mut registry, cap.master_id(), scenario.ctx());
    assert!(forwarding_address::pending_rotation_for_testing(&registry, cap.master_id()).is_none());
    test_scenario::return_shared(registry);
    transfer::public_transfer(cap, ALICE);
    scenario.end();
}

#[test, expected_failure(abort_code = sui::forwarding_address::ENoPendingRotation)]
fun cancelling_without_a_pending_rotation_aborts() {
    let (scenario, cap) = register_alice_then_switch_to_bob();
    let mut registry = scenario.take_shared<ForwardingAddressRegistry>();
    forwarding_address::cancel_rotation(&mut registry, &cap);
    test_scenario::return_shared(registry);
    transfer::public_transfer(cap, ALICE);
    scenario.end();
}

#[test, expected_failure(abort_code = sui::forwarding_address::EForwardingAddressMaster)]
fun a_forwarding_address_cannot_become_a_master() {
    let (mut scenario, cap) = register_alice_then_switch_to_bob();
    let mut registry = scenario.take_shared<ForwardingAddressRegistry>();
    let target = forwarding_address(cap.master_id(), OPAQUE_VARIANT, 1);
    forwarding_address::propose_rotation(&mut registry, &cap, target, scenario.ctx());
    test_scenario::return_shared(registry);
    transfer::public_transfer(cap, ALICE);
    scenario.end();
}

#[test, expected_failure(abort_code = sui::forwarding_address::EInvalidRotationDelay)]
fun registering_with_no_delay_aborts() {
    let mut scenario = test_scenario::begin(@0x0);
    forwarding_address::create_for_testing(scenario.ctx());
    scenario.next_tx(ALICE);
    let mut registry = scenario.take_shared<ForwardingAddressRegistry>();
    let cap = forwarding_address::register(&mut registry, 0, scenario.ctx());
    transfer::public_transfer(cap, ALICE);
    test_scenario::return_shared(registry);
    scenario.end();
}

#[test, expected_failure(abort_code = sui::test_scenario::EForwardingAddressUnresolvable)]
fun deposits_through_more_than_the_allowed_hops_abort() {
    let mut scenario = test_scenario::begin(@0x0);
    forwarding_address::create_for_testing(scenario.ctx());
    // Four hops to ALICE, one more than `forwarding_address_max_hops` allows.
    let mut registrant = ALICE;
    let mut i: u64 = 0;
    while (i < 4) {
        let id = register_from(&mut scenario, registrant);
        registrant = forwarding_address(id, OPAQUE_VARIANT, 1);
        i = i + 1;
    };
    scenario.next_tx(BOB);
    balance::create_for_testing<SUI>(1000).send_funds(registrant);
    scenario.next_tx(BOB);
    scenario.end();
}

#[test]
fun the_rotation_delay_can_only_grow() {
    let (scenario, cap) = register_alice_then_switch_to_bob();
    let mut registry = scenario.take_shared<ForwardingAddressRegistry>();
    forwarding_address::increase_rotation_delay(&mut registry, &cap, 5);
    test_scenario::return_shared(registry);
    transfer::public_transfer(cap, ALICE);
    scenario.end();
}

#[test, expected_failure(abort_code = sui::forwarding_address::EInvalidRotationDelay)]
fun shortening_the_rotation_delay_aborts() {
    let (scenario, cap) = register_alice_then_switch_to_bob();
    let mut registry = scenario.take_shared<ForwardingAddressRegistry>();
    forwarding_address::increase_rotation_delay(&mut registry, &cap, 1);
    test_scenario::return_shared(registry);
    transfer::public_transfer(cap, ALICE);
    scenario.end();
}

#[test]
fun objects_sent_to_a_forwarding_address_reach_the_master() {
    let (mut scenario, master_id) = register_then_switch_to(ALICE, BOB);
    let target = forwarding_address(master_id, OPAQUE_VARIANT, 1);
    let coin = sui::coin::mint_for_testing<SUI>(500, scenario.ctx());
    let coin_id = object::id(&coin);
    transfer::public_transfer(coin, target);

    scenario.next_tx(ALICE);
    let coin = scenario.take_from_sender_by_id<sui::coin::Coin<SUI>>(coin_id);
    assert!(coin.value() == 500);
    scenario.return_to_sender(coin);
    scenario.end();
}

#[test, expected_failure(abort_code = sui::test_scenario::EForwardingAddressUnresolvable)]
fun objects_sent_to_an_unregistered_forwarding_address_abort() {
    let (mut scenario, _) = register_then_switch_to(ALICE, BOB);
    let unregistered = forwarding_address::mix_master_id_for_testing(2);
    transfer::public_transfer(
        sui::coin::mint_for_testing<SUI>(500, scenario.ctx()),
        forwarding_address(unregistered, OPAQUE_VARIANT, 1),
    );
    scenario.next_tx(BOB);
    scenario.end();
}
