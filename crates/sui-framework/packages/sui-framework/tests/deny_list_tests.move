// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::deny_list_tests;

use std::type_name;
use sui::deny_list;
use sui::test_scenario;

public struct X()

#[test, expected_failure(abort_code = sui::deny_list::EInvalidAddress)]
fun add_zero() {
    let mut ctx = tx_context::dummy();
    let mut dl = deny_list::new_for_testing(&mut ctx);
    let ty = type_name::into_string(type_name::with_original_ids<X>()).into_bytes();
    dl.v1_add(1, ty, deny_list::reserved_addresses()[0]); // should error
    abort 0 // should not be reached
}

#[test, expected_failure(abort_code = sui::deny_list::EInvalidAddress)]
fun remove_zero() {
    let mut ctx = tx_context::dummy();
    let mut dl = deny_list::new_for_testing(&mut ctx);
    let ty = type_name::into_string(type_name::with_original_ids<X>()).into_bytes();
    dl.v1_add(1, ty, deny_list::reserved_addresses()[1]); // should error
    abort 0 // should not be reached
}

#[test]
fun contains_zero() {
    let mut scenario = test_scenario::begin(@0);
    deny_list::create_for_testing(scenario.ctx());
    scenario.next_tx(@0);
    let dl: deny_list::DenyList = scenario.take_shared();
    let ty = type_name::into_string(type_name::with_original_ids<X>()).into_bytes();
    let reserved = deny_list::reserved_addresses();
    let mut i = 0;
    let n = reserved.length();
    while (i < n) {
        assert!(!dl.v1_contains(1, ty, reserved[i]));
        i = i + 1;
    };
    test_scenario::return_shared(dl);
    scenario.end();
}

// === Seal / activate ===

const SLOTS: u64 = 4;

fun setup_active(scenario: &mut test_scenario::Scenario) {
    deny_list::create_for_testing(scenario.ctx());
    scenario.next_tx(@0);
    let mut dl: deny_list::DenyList = scenario.take_shared();
    dl.create_active_for_testing(SLOTS, scenario.ctx());
    test_scenario::return_shared(dl);
    scenario.next_tx(@0);
}

fun take_staging(
    scenario: &test_scenario::Scenario,
    generation: u64,
): deny_list::DenyListStaging {
    scenario.take_shared_by_id(deny_list::staging_slot_address(generation % SLOTS).to_id())
}

#[test]
fun seal_then_activate() {
    let mut scenario = test_scenario::begin(@0);
    setup_active(&mut scenario);
    let ty = type_name::into_string(type_name::with_original_ids<X>()).into_bytes();
    let epoch = scenario.ctx().epoch();

    let mut dl: deny_list::DenyList = scenario.take_shared();
    dl.v2_add(0, ty, @0x100, scenario.ctx());
    dl.v2_enable_global_pause(0, ty, scenario.ctx());
    let mut staging = take_staging(&scenario, 1);
    dl.seal_for_testing(&mut staging, epoch, 1, scenario.ctx());
    test_scenario::return_shared(staging);
    test_scenario::return_shared(dl);
    scenario.next_tx(@0);

    // Sealed but not activated: nothing is in effect yet.
    let mut active: deny_list::ActiveDenyList = scenario.take_shared();
    assert!(!active.active_contains_address(0, ty, @0x100));
    assert!(!active.active_global_pause_enabled(0, ty));
    let staging = take_staging(&scenario, 1);
    active.activate_for_testing(&staging, epoch, 1, scenario.ctx());
    assert!(active.active_contains_address(0, ty, @0x100));
    assert!(active.active_global_pause_enabled(0, ty));
    assert!(!active.active_contains_address(0, ty, @0x101));
    test_scenario::return_shared(staging);
    test_scenario::return_shared(active);
    scenario.next_tx(@0);

    // Removal goes through the same path and deletes the entry.
    let mut dl: deny_list::DenyList = scenario.take_shared();
    dl.v2_remove(0, ty, @0x100, scenario.ctx());
    dl.v2_disable_global_pause(0, ty, scenario.ctx());
    let mut staging = take_staging(&scenario, 2);
    dl.seal_for_testing(&mut staging, epoch, 2, scenario.ctx());
    test_scenario::return_shared(staging);
    test_scenario::return_shared(dl);
    scenario.next_tx(@0);

    let mut active: deny_list::ActiveDenyList = scenario.take_shared();
    let staging = take_staging(&scenario, 2);
    active.activate_for_testing(&staging, epoch, 2, scenario.ctx());
    assert!(!active.active_contains_address(0, ty, @0x100));
    assert!(!active.active_global_pause_enabled(0, ty));
    test_scenario::return_shared(staging);
    test_scenario::return_shared(active);
    scenario.end();
}

#[test]
fun flush_applies_unsealed_updates() {
    let mut scenario = test_scenario::begin(@0);
    setup_active(&mut scenario);
    let ty = type_name::into_string(type_name::with_original_ids<X>()).into_bytes();
    let epoch = scenario.ctx().epoch();

    let mut dl: deny_list::DenyList = scenario.take_shared();
    dl.v2_add(0, ty, @0x100, scenario.ctx());
    let mut active: deny_list::ActiveDenyList = scenario.take_shared();
    dl.flush_pending_for_testing(&mut active, epoch, scenario.ctx());
    assert!(active.active_contains_address(0, ty, @0x100));
    // The pending list was drained, so a later seal carries nothing.
    let mut staging = take_staging(&scenario, 1);
    dl.seal_for_testing(&mut staging, epoch, 1, scenario.ctx());
    dl.v2_remove(0, ty, @0x100, scenario.ctx());
    active.activate_for_testing(&staging, epoch, 1, scenario.ctx());
    assert!(active.active_contains_address(0, ty, @0x100));
    test_scenario::return_shared(staging);
    test_scenario::return_shared(active);
    test_scenario::return_shared(dl);
    scenario.end();
}

#[test, expected_failure(abort_code = sui::deny_list::EWrongGeneration)]
fun activate_wrong_generation() {
    let mut scenario = test_scenario::begin(@0);
    setup_active(&mut scenario);
    let epoch = scenario.ctx().epoch();
    let mut dl: deny_list::DenyList = scenario.take_shared();
    let mut staging = take_staging(&scenario, 1);
    dl.seal_for_testing(&mut staging, epoch, 1, scenario.ctx());
    let mut active: deny_list::ActiveDenyList = scenario.take_shared();
    active.activate_for_testing(&staging, epoch, 2, scenario.ctx());
    abort 0
}

#[test]
fun writes_before_create_active_are_not_recorded() {
    let mut scenario = test_scenario::begin(@0);
    deny_list::create_for_testing(scenario.ctx());
    scenario.next_tx(@0);
    let ty = type_name::into_string(type_name::with_original_ids<X>()).into_bytes();
    let epoch = scenario.ctx().epoch();
    let mut dl: deny_list::DenyList = scenario.take_shared();
    dl.v2_add(0, ty, @0x100, scenario.ctx());
    dl.create_active_for_testing(SLOTS, scenario.ctx());
    test_scenario::return_shared(dl);
    scenario.next_tx(@0);
    let mut dl: deny_list::DenyList = scenario.take_shared();
    let mut active: deny_list::ActiveDenyList = scenario.take_shared();
    dl.flush_pending_for_testing(&mut active, epoch, scenario.ctx());
    assert!(!active.active_contains_address(0, ty, @0x100));
    test_scenario::return_shared(active);
    test_scenario::return_shared(dl);
    scenario.end();
}
