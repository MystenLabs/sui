// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::coin_regulation_tests;

use std::unit_test::{assert_eq, destroy};
use sui::coin_regulation::{
    Self,
    FreezeList,
    ComplianceCap,
    SeizeAuth,
    AddressFrozen,
    AddressUnfrozen,
    AddressPermanentlyFrozen,
    SeizeAuthIssued,
    SeizeAuthDestroyed,
    CoinTypePaused,
    CoinTypeUnpaused,
};
use sui::event;
use sui::test_scenario::{Self, Scenario};

public struct TEST()

public struct OTHER()

const ISSUER: address = @0x11;
const ALICE: address = @0xA;
const BOB: address = @0xB;

fun setup(allow_global_pause: bool): (Scenario, FreezeList, ComplianceCap<TEST>) {
    let mut scenario = test_scenario::begin(ISSUER);
    scenario.create_system_objects();
    let list = scenario.take_shared<FreezeList>();
    let cap = coin_regulation::new_cap_for_testing<TEST>(allow_global_pause, scenario.ctx());
    (scenario, list, cap)
}

fun next_epoch(scenario: &mut Scenario, list: FreezeList): FreezeList {
    test_scenario::return_shared(list);
    scenario.next_epoch(ISSUER);
    scenario.take_shared<FreezeList>()
}

fun teardown(scenario: Scenario, list: FreezeList, cap: ComplianceCap<TEST>) {
    test_scenario::return_shared(list);
    destroy(cap);
    scenario.end();
}

#[test]
fun freeze_and_unfreeze() {
    let (mut scenario, mut list, mut cap) = setup(false);
    assert!(!list.is_frozen<TEST>(ALICE));
    assert_eq!(list.num_frozen_addresses<TEST>(), 0);

    list.freeze_address(&mut cap, ALICE, scenario.ctx());
    list.freeze_address(&mut cap, BOB, scenario.ctx());
    assert!(list.is_frozen<TEST>(ALICE));
    assert!(list.is_frozen<TEST>(BOB));
    assert!(!list.is_permanently_frozen<TEST>(ALICE));
    assert!(list.seize_auths<TEST>(ALICE).is_empty());
    assert_eq!(list.num_frozen_addresses<TEST>(), 2);

    list.unfreeze_address(&mut cap, ALICE, scenario.ctx());
    assert!(!list.is_frozen<TEST>(ALICE));
    assert!(list.is_frozen<TEST>(BOB));
    assert_eq!(list.num_frozen_addresses<TEST>(), 1);

    assert_eq!(event::events_by_type<AddressFrozen<TEST>>().length(), 2);
    assert_eq!(event::events_by_type<AddressUnfrozen<TEST>>().length(), 1);

    teardown(scenario, list, cap);
}

#[test]
fun freeze_adds_coin_type_state() {
    let (mut scenario, mut list, mut cap) = setup(false);
    assert!(!list.has_coin_type_state<TEST>());
    list.freeze_address(&mut cap, ALICE, scenario.ctx());
    assert!(list.has_coin_type_state<TEST>());
    assert!(!list.is_global_pause<TEST>());
    list.unfreeze_address(&mut cap, ALICE, scenario.ctx());
    assert!(list.has_coin_type_state<TEST>());
    assert_eq!(list.num_frozen_addresses<TEST>(), 0);
    teardown(scenario, list, cap);
}

#[test]
fun freeze_is_per_coin_type() {
    let (mut scenario, mut list, mut cap) = setup(false);
    let mut other_cap = coin_regulation::new_cap_for_testing<OTHER>(false, scenario.ctx());

    list.freeze_address(&mut cap, ALICE, scenario.ctx());
    assert!(list.is_frozen<TEST>(ALICE));
    assert!(!list.is_frozen<OTHER>(ALICE));
    assert_eq!(list.num_frozen_addresses<OTHER>(), 0);

    list.freeze_address(&mut other_cap, ALICE, scenario.ctx());
    list.unfreeze_address(&mut cap, ALICE, scenario.ctx());
    assert!(!list.is_frozen<TEST>(ALICE));
    assert!(list.is_frozen<OTHER>(ALICE));
    assert_eq!(list.num_frozen_addresses<TEST>(), 0);
    assert_eq!(list.num_frozen_addresses<OTHER>(), 1);

    destroy(other_cap);
    teardown(scenario, list, cap);
}

#[test, expected_failure(abort_code = coin_regulation::EAlreadyFrozen)]
fun freeze_twice_aborts() {
    let (mut scenario, mut list, mut cap) = setup(false);
    list.freeze_address(&mut cap, ALICE, scenario.ctx());
    list.freeze_address(&mut cap, ALICE, scenario.ctx());
    abort
}

#[test, expected_failure(abort_code = coin_regulation::ENotFrozen)]
fun unfreeze_unfrozen_aborts() {
    let (mut scenario, mut list, mut cap) = setup(false);
    list.unfreeze_address(&mut cap, ALICE, scenario.ctx());
    abort
}

#[test]
fun seize_auths_of_unfrozen_is_empty() {
    let (scenario, list, cap) = setup(false);
    assert!(list.seize_auths<TEST>(ALICE).is_empty());
    teardown(scenario, list, cap);
}

#[test]
fun permanently_freeze() {
    let (mut scenario, mut list, mut cap) = setup(false);
    list.freeze_address(&mut cap, ALICE, scenario.ctx());
    list.permanently_freeze_address(&mut cap, ALICE, scenario.ctx());
    assert!(list.is_frozen<TEST>(ALICE));
    assert!(list.is_permanently_frozen<TEST>(ALICE));
    assert_eq!(list.num_frozen_addresses<TEST>(), 1);
    assert_eq!(event::events_by_type<AddressPermanentlyFrozen<TEST>>().length(), 1);
    teardown(scenario, list, cap);
}

#[test, expected_failure(abort_code = coin_regulation::EPermanentlyFrozen)]
fun unfreeze_permanent_aborts() {
    let (mut scenario, mut list, mut cap) = setup(false);
    list.freeze_address(&mut cap, ALICE, scenario.ctx());
    list.permanently_freeze_address(&mut cap, ALICE, scenario.ctx());
    list.unfreeze_address(&mut cap, ALICE, scenario.ctx());
    abort
}

#[test]
fun permanently_freeze_unfrozen() {
    let (mut scenario, mut list, mut cap) = setup(false);
    list.permanently_freeze_address(&mut cap, ALICE, scenario.ctx());
    assert!(list.is_frozen<TEST>(ALICE));
    assert!(list.is_permanently_frozen<TEST>(ALICE));
    assert_eq!(list.num_frozen_addresses<TEST>(), 1);
    let auth = list.issue_seize_auth(&mut cap, ALICE, scenario.ctx());
    assert_eq!(auth.frozen_epoch(), 0);
    list.destroy_seize_auth(auth, scenario.ctx());
    assert_eq!(event::events_by_type<AddressFrozen<TEST>>().length(), 1);
    assert_eq!(event::events_by_type<AddressPermanentlyFrozen<TEST>>().length(), 1);
    teardown(scenario, list, cap);
}

#[test, expected_failure(abort_code = coin_regulation::EPermanentlyFrozen)]
fun unfreeze_permanently_frozen_unfrozen_aborts() {
    let (mut scenario, mut list, mut cap) = setup(false);
    list.permanently_freeze_address(&mut cap, ALICE, scenario.ctx());
    list.unfreeze_address(&mut cap, ALICE, scenario.ctx());
    abort
}

#[test, expected_failure(abort_code = coin_regulation::ENotFrozen)]
fun issue_seize_auth_unfrozen_aborts() {
    let (mut scenario, mut list, mut cap) = setup(false);
    let _auth = list.issue_seize_auth(&mut cap, ALICE, scenario.ctx());
    abort
}

#[test, expected_failure(abort_code = coin_regulation::ESeizeAuthsOutstanding)]
fun unfreeze_with_seize_auth_aborts() {
    let (mut scenario, mut list, mut cap) = setup(false);
    list.freeze_address(&mut cap, ALICE, scenario.ctx());
    let _auth = list.issue_seize_auth(&mut cap, ALICE, scenario.ctx());
    list.unfreeze_address(&mut cap, ALICE, scenario.ctx());
    abort
}

#[test]
fun unfreeze_after_seize_auths_destroyed() {
    let (mut scenario, mut list, mut cap) = setup(false);
    list.freeze_address(&mut cap, ALICE, scenario.ctx());
    let auth1 = list.issue_seize_auth(&mut cap, ALICE, scenario.ctx());
    let auth2 = list.issue_seize_auth(&mut cap, ALICE, scenario.ctx());
    assert_eq!(auth1.frozen_address(), ALICE);
    assert_eq!(list.seize_auths<TEST>(ALICE).length(), 2);

    list.destroy_seize_auth(auth1, scenario.ctx());
    list.destroy_seize_auth(auth2, scenario.ctx());
    assert!(list.seize_auths<TEST>(ALICE).is_empty());
    assert_eq!(event::events_by_type<SeizeAuthIssued<TEST>>().length(), 2);
    assert_eq!(event::events_by_type<SeizeAuthDestroyed<TEST>>().length(), 2);

    let mut list = next_epoch(&mut scenario, list);
    list.unfreeze_address(&mut cap, ALICE, scenario.ctx());
    assert!(!list.is_frozen<TEST>(ALICE));
    assert_eq!(list.num_frozen_addresses<TEST>(), 0);
    teardown(scenario, list, cap);
}

#[test]
fun unfreeze_in_freeze_epoch_zero() {
    let (mut scenario, mut list, mut cap) = setup(false);
    assert_eq!(scenario.ctx().epoch(), 0);
    list.freeze_address(&mut cap, ALICE, scenario.ctx());
    list.unfreeze_address(&mut cap, ALICE, scenario.ctx());
    assert!(!list.is_frozen<TEST>(ALICE));
    teardown(scenario, list, cap);
}

#[test, expected_failure(abort_code = coin_regulation::EUnfreezeTooEarly)]
fun unfreeze_in_destroy_epoch_aborts() {
    let (mut scenario, mut list, mut cap) = setup(false);
    list.freeze_address(&mut cap, ALICE, scenario.ctx());
    let mut list = next_epoch(&mut scenario, list);
    let auth = list.issue_seize_auth(&mut cap, ALICE, scenario.ctx());
    list.destroy_seize_auth(auth, scenario.ctx());
    list.unfreeze_address(&mut cap, ALICE, scenario.ctx());
    abort
}

#[test]
fun unfreeze_in_epoch_after_destroy() {
    let (mut scenario, mut list, mut cap) = setup(false);
    list.freeze_address(&mut cap, ALICE, scenario.ctx());
    let mut list = next_epoch(&mut scenario, list);
    let auth = list.issue_seize_auth(&mut cap, ALICE, scenario.ctx());
    list.destroy_seize_auth(auth, scenario.ctx());
    let mut list = next_epoch(&mut scenario, list);
    list.unfreeze_address(&mut cap, ALICE, scenario.ctx());
    assert!(!list.is_frozen<TEST>(ALICE));
    teardown(scenario, list, cap);
}

#[test]
fun destroy_one_of_two_seize_auths() {
    let (mut scenario, mut list, mut cap) = setup(false);
    list.freeze_address(&mut cap, ALICE, scenario.ctx());
    let auth1 = list.issue_seize_auth(&mut cap, ALICE, scenario.ctx());
    let auth2 = list.issue_seize_auth(&mut cap, ALICE, scenario.ctx());
    let id2 = object::id(&auth2);

    list.destroy_seize_auth(auth1, scenario.ctx());
    assert_eq!(list.seize_auths<TEST>(ALICE), vector[id2]);
    assert!(list.is_frozen<TEST>(ALICE));

    list.destroy_seize_auth(auth2, scenario.ctx());
    teardown(scenario, list, cap);
}

#[test, expected_failure(abort_code = coin_regulation::ESeizeAuthsOutstanding)]
fun unfreeze_after_destroying_one_of_two_aborts() {
    let (mut scenario, mut list, mut cap) = setup(false);
    list.freeze_address(&mut cap, ALICE, scenario.ctx());
    let auth1 = list.issue_seize_auth(&mut cap, ALICE, scenario.ctx());
    let _auth2 = list.issue_seize_auth(&mut cap, ALICE, scenario.ctx());
    list.destroy_seize_auth(auth1, scenario.ctx());
    list.unfreeze_address(&mut cap, ALICE, scenario.ctx());
    abort
}

#[test]
fun permanent_freeze_with_seize_auths() {
    let (mut scenario, mut list, mut cap) = setup(false);
    list.freeze_address(&mut cap, ALICE, scenario.ctx());
    let auth1 = list.issue_seize_auth(&mut cap, ALICE, scenario.ctx());
    list.permanently_freeze_address(&mut cap, ALICE, scenario.ctx());
    let auth2 = list.issue_seize_auth(&mut cap, ALICE, scenario.ctx());
    assert_eq!(list.seize_auths<TEST>(ALICE).length(), 2);

    list.destroy_seize_auth(auth1, scenario.ctx());
    list.destroy_seize_auth(auth2, scenario.ctx());
    assert!(list.seize_auths<TEST>(ALICE).is_empty());
    assert!(list.is_permanently_frozen<TEST>(ALICE));
    assert_eq!(list.num_frozen_addresses<TEST>(), 1);
    teardown(scenario, list, cap);
}

#[test, expected_failure(abort_code = coin_regulation::EPermanentlyFrozen)]
fun unfreeze_permanent_after_seize_auths_destroyed_aborts() {
    let (mut scenario, mut list, mut cap) = setup(false);
    list.freeze_address(&mut cap, ALICE, scenario.ctx());
    let auth = list.issue_seize_auth(&mut cap, ALICE, scenario.ctx());
    list.permanently_freeze_address(&mut cap, ALICE, scenario.ctx());
    list.destroy_seize_auth(auth, scenario.ctx());
    list.unfreeze_address(&mut cap, ALICE, scenario.ctx());
    abort
}

#[test]
fun transfer_seize_auth() {
    let (mut scenario, mut list, mut cap) = setup(false);
    list.freeze_address(&mut cap, ALICE, scenario.ctx());
    let auth = list.issue_seize_auth(&mut cap, ALICE, scenario.ctx());
    let id = object::id(&auth);
    coin_regulation::transfer_seize_auth(auth, BOB);
    test_scenario::return_shared(list);

    scenario.next_tx(BOB);
    let mut list = scenario.take_shared<FreezeList>();
    let auth = scenario.take_from_sender<SeizeAuth<TEST>>();
    assert_eq!(object::id(&auth), id);
    list.destroy_seize_auth(auth, scenario.ctx());
    assert!(list.seize_auths<TEST>(ALICE).is_empty());
    teardown(scenario, list, cap);
}

#[test]
fun seize_auth_copies_frozen_epoch() {
    let (mut scenario, mut list, mut cap) = setup(false);
    list.freeze_address(&mut cap, ALICE, scenario.ctx());
    let mut list = next_epoch(&mut scenario, list);
    list.freeze_address(&mut cap, BOB, scenario.ctx());
    let alice_auth = list.issue_seize_auth(&mut cap, ALICE, scenario.ctx());
    let bob_auth = list.issue_seize_auth(&mut cap, BOB, scenario.ctx());
    assert_eq!(alice_auth.frozen_epoch(), 0);
    assert_eq!(bob_auth.frozen_epoch(), 1);
    list.destroy_seize_auth(alice_auth, scenario.ctx());
    list.destroy_seize_auth(bob_auth, scenario.ctx());
    teardown(scenario, list, cap);
}

#[test]
fun pause_and_unpause() {
    let (mut scenario, mut list, mut cap) = setup(true);
    assert!(cap.allow_global_pause());
    assert!(!list.is_global_pause<TEST>());
    assert!(!list.is_global_pause<OTHER>());

    list.enable_global_pause(&mut cap, scenario.ctx());
    assert!(list.is_global_pause<TEST>());
    assert!(!list.is_global_pause<OTHER>());

    list.disable_global_pause(&mut cap, scenario.ctx());
    assert!(!list.is_global_pause<TEST>());

    assert_eq!(event::events_by_type<CoinTypePaused<TEST>>().length(), 1);
    assert_eq!(event::events_by_type<CoinTypeUnpaused<TEST>>().length(), 1);
    teardown(scenario, list, cap);
}

#[test]
fun pause_adds_coin_type_state() {
    let (mut scenario, mut list, mut cap) = setup(true);
    assert!(!list.has_coin_type_state<TEST>());
    list.enable_global_pause(&mut cap, scenario.ctx());
    assert!(list.has_coin_type_state<TEST>());
    assert!(list.is_global_pause<TEST>());
    assert_eq!(list.num_frozen_addresses<TEST>(), 0);
    teardown(scenario, list, cap);
}

#[test]
fun unpause_without_coin_type_state() {
    let (mut scenario, mut list, mut cap) = setup(true);
    list.disable_global_pause(&mut cap, scenario.ctx());
    assert!(!list.has_coin_type_state<TEST>());
    assert!(!list.is_global_pause<TEST>());
    assert_eq!(event::events_by_type<CoinTypeUnpaused<TEST>>().length(), 1);
    teardown(scenario, list, cap);
}

#[test, expected_failure(abort_code = coin_regulation::EGlobalPauseNotAllowed)]
fun pause_without_allow_global_pause_aborts() {
    let (mut scenario, mut list, mut cap) = setup(false);
    list.enable_global_pause(&mut cap, scenario.ctx());
    abort
}

#[test, expected_failure(abort_code = coin_regulation::EGlobalPauseNotAllowed)]
fun unpause_without_allow_global_pause_aborts() {
    let (mut scenario, mut list, mut cap) = setup(false);
    list.disable_global_pause(&mut cap, scenario.ctx());
    abort
}
