// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::coin_regulation_tests;

use std::unit_test::{assert_eq, destroy};
use sui::coin::{Self, DenyCapV2};
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
    DenyCapMigrated,
};
use sui::deny_list::DenyList;
use sui::event;
use sui::test_scenario::{Self, Scenario};

public struct TEST()

public struct OTHER()

const ISSUER: address = @0x11;
const ALICE: address = @0xA;
const BOB: address = @0xB;
const CAROL: address = @0xC0;
const DAVE: address = @0xD0;
const ERIN: address = @0xE0;

fun setup(allow_global_pause: bool): (Scenario, FreezeList, ComplianceCap<TEST>) {
    let mut scenario = test_scenario::begin(ISSUER);
    scenario.create_system_objects();
    let list = scenario.take_shared<FreezeList>();
    let cap = coin_regulation::new_compliance_cap<TEST>(allow_global_pause, scenario.ctx());
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

fun address_n(n: u64): address {
    sui::address::from_u256(0x1000 + (n as u256))
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
    assert!(!list.is_global_pause_enabled<TEST>());
    list.unfreeze_address(&mut cap, ALICE, scenario.ctx());
    assert!(list.has_coin_type_state<TEST>());
    assert_eq!(list.num_frozen_addresses<TEST>(), 0);
    teardown(scenario, list, cap);
}

#[test]
fun freeze_is_per_coin_type() {
    let (mut scenario, mut list, mut cap) = setup(false);
    let mut other_cap = coin_regulation::new_compliance_cap<OTHER>(false, scenario.ctx());

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

#[test]
fun freeze_limit_resets_each_transaction() {
    let (mut scenario, mut list, mut cap) = setup(false);
    8u64.do!(|i| list.freeze_address(&mut cap, address_n(i), scenario.ctx()));
    scenario.next_tx(ISSUER);
    list.freeze_address(&mut cap, address_n(8), scenario.ctx());
    assert_eq!(list.num_frozen_addresses<TEST>(), 9);
    teardown(scenario, list, cap);
}

#[test, expected_failure(abort_code = coin_regulation::ETooManyFreezes)]
fun freeze_over_limit_aborts() {
    let (mut scenario, mut list, mut cap) = setup(false);
    9u64.do!(|i| list.freeze_address(&mut cap, address_n(i), scenario.ctx()));
    teardown(scenario, list, cap);
}

#[test, expected_failure(abort_code = coin_regulation::ETooManyFreezes)]
fun permanently_freeze_new_address_counts_toward_limit() {
    let (mut scenario, mut list, mut cap) = setup(false);
    8u64.do!(|i| list.freeze_address(&mut cap, address_n(i), scenario.ctx()));
    list.permanently_freeze_address(&mut cap, ALICE, scenario.ctx());
    teardown(scenario, list, cap);
}

#[test]
fun permanently_freeze_frozen_address_does_not_count() {
    let (mut scenario, mut list, mut cap) = setup(false);
    8u64.do!(|i| list.freeze_address(&mut cap, address_n(i), scenario.ctx()));
    8u64.do!(|i| list.permanently_freeze_address(&mut cap, address_n(i), scenario.ctx()));
    assert!(list.is_permanently_frozen<TEST>(address_n(0)));
    teardown(scenario, list, cap);
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

#[test, expected_failure(abort_code = coin_regulation::EPermanentlyFrozen)]
fun permanently_freeze_after_seize_auth_destroyed() {
    let (mut scenario, mut list, mut cap) = setup(false);
    list.freeze_address(&mut cap, ALICE, scenario.ctx());
    let auth = list.issue_seize_auth(&mut cap, ALICE, scenario.ctx());
    list.destroy_seize_auth(auth, scenario.ctx());
    list.permanently_freeze_address(&mut cap, ALICE, scenario.ctx());
    assert!(list.is_permanently_frozen<TEST>(ALICE));
    list = next_epoch(&mut scenario, list);
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
    assert!(!list.is_global_pause_enabled<TEST>());
    assert!(!list.is_global_pause_enabled<OTHER>());

    list.enable_global_pause(&mut cap, scenario.ctx());
    assert!(list.is_global_pause_enabled<TEST>());
    assert!(!list.is_global_pause_enabled<OTHER>());

    list.disable_global_pause(&mut cap, scenario.ctx());
    assert!(!list.is_global_pause_enabled<TEST>());

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
    assert!(list.is_global_pause_enabled<TEST>());
    assert_eq!(list.num_frozen_addresses<TEST>(), 0);
    teardown(scenario, list, cap);
}

#[test]
fun unpause_without_coin_type_state() {
    let (mut scenario, mut list, mut cap) = setup(true);
    list.disable_global_pause(&mut cap, scenario.ctx());
    assert!(!list.has_coin_type_state<TEST>());
    assert!(!list.is_global_pause_enabled<TEST>());
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

fun setup_deny_list(allow_global_pause: bool): (Scenario, DenyCapV2<TEST>) {
    let mut scenario = test_scenario::begin(ISSUER);
    scenario.create_system_objects();
    let cap = coin::new_deny_cap_v2<TEST>(allow_global_pause, scenario.ctx());
    (scenario, cap)
}

fun migrate(scenario: &mut Scenario, cap: DenyCapV2<TEST>, addresses: vector<address>) {
    scenario.next_tx(ISSUER);
    let mut list = scenario.take_shared<FreezeList>();
    let mut deny_list = scenario.take_shared<DenyList>();
    let cap = list.migrate_from_deny_list(&mut deny_list, cap, addresses, scenario.ctx());
    test_scenario::return_shared(list);
    test_scenario::return_shared(deny_list);
    transfer::public_transfer(cap, ISSUER);
}

#[test]
fun migrate_keeps_cap_id_and_global_pause() {
    let (mut scenario, cap) = setup_deny_list(true);
    let cap_id = object::id(&cap);
    let mut list = scenario.take_shared<FreezeList>();
    let mut deny_list = scenario.take_shared<DenyList>();
    let cap = list.migrate_from_deny_list(&mut deny_list, cap, vector[], scenario.ctx());
    assert_eq!(object::id(&cap), cap_id);
    assert!(cap.allow_global_pause());
    assert!(!list.is_global_pause_enabled<TEST>());
    assert_eq!(list.num_frozen_addresses<TEST>(), 0);
    let events = event::events_by_type<DenyCapMigrated<TEST>>();
    assert_eq!(events.length(), 1);

    let other_cap = coin::new_deny_cap_v2<OTHER>(false, scenario.ctx());
    let other_id = object::id(&other_cap);
    let other_cap = list.migrate_from_deny_list(
        &mut deny_list,
        other_cap,
        vector[],
        scenario.ctx(),
    );
    assert_eq!(object::id(&other_cap), other_id);
    assert!(!other_cap.allow_global_pause());

    destroy(other_cap);
    test_scenario::return_shared(deny_list);
    teardown(scenario, list, cap);
}

#[test]
fun migrate_freezes_denied_addresses() {
    let (mut scenario, mut cap) = setup_deny_list(false);
    let mut deny_list = scenario.take_shared<DenyList>();
    coin::deny_list_v2_add(&mut deny_list, &mut cap, ALICE, scenario.ctx());
    coin::deny_list_v2_add(&mut deny_list, &mut cap, BOB, scenario.ctx());
    coin::deny_list_v2_add(&mut deny_list, &mut cap, ERIN, scenario.ctx());
    test_scenario::return_shared(deny_list);
    scenario.next_epoch(ISSUER);

    let mut deny_list = scenario.take_shared<DenyList>();
    // CAROL is a pending add and BOB a pending removal.
    coin::deny_list_v2_add(&mut deny_list, &mut cap, CAROL, scenario.ctx());
    coin::deny_list_v2_remove(&mut deny_list, &mut cap, BOB, scenario.ctx());
    assert!(!coin::deny_list_v2_contains_current_epoch<TEST>(&deny_list, CAROL, scenario.ctx()));
    assert!(coin::deny_list_v2_contains_current_epoch<TEST>(&deny_list, BOB, scenario.ctx()));
    test_scenario::return_shared(deny_list);

    migrate(&mut scenario, cap, vector[ALICE, BOB, CAROL, DAVE, ALICE]);
    scenario.next_tx(ISSUER);

    let list = scenario.take_shared<FreezeList>();
    assert!(list.is_frozen<TEST>(ALICE));
    assert!(list.is_frozen<TEST>(CAROL));
    assert!(!list.is_frozen<TEST>(BOB));
    assert!(!list.is_frozen<TEST>(DAVE));
    // Denied but omitted from the migration.
    assert!(!list.is_frozen<TEST>(ERIN));
    assert!(!list.is_frozen<OTHER>(ALICE));
    assert_eq!(list.num_frozen_addresses<TEST>(), 2);
    let cap = scenario.take_from_sender<ComplianceCap<TEST>>();
    teardown(scenario, list, cap);
}

#[test]
fun migrate_records_migration_epoch() {
    let (mut scenario, mut cap) = setup_deny_list(false);
    let mut deny_list = scenario.take_shared<DenyList>();
    coin::deny_list_v2_add(&mut deny_list, &mut cap, ALICE, scenario.ctx());
    test_scenario::return_shared(deny_list);
    scenario.next_epoch(ISSUER);

    migrate(&mut scenario, cap, vector[ALICE]);
    scenario.next_tx(ISSUER);

    let mut list = scenario.take_shared<FreezeList>();
    let mut cap = scenario.take_from_sender<ComplianceCap<TEST>>();
    let auth = list.issue_seize_auth(&mut cap, ALICE, scenario.ctx());
    assert_eq!(auth.frozen_epoch(), 1);
    list.destroy_seize_auth(auth, scenario.ctx());
    teardown(scenario, list, cap);
}

#[test]
fun unfreeze_in_migration_epoch() {
    let (mut scenario, mut cap) = setup_deny_list(false);
    let mut deny_list = scenario.take_shared<DenyList>();
    coin::deny_list_v2_add(&mut deny_list, &mut cap, ALICE, scenario.ctx());
    test_scenario::return_shared(deny_list);
    scenario.next_epoch(ISSUER);

    migrate(&mut scenario, cap, vector[ALICE]);
    scenario.next_tx(ISSUER);

    let mut list = scenario.take_shared<FreezeList>();
    let mut cap = scenario.take_from_sender<ComplianceCap<TEST>>();
    assert!(list.is_frozen<TEST>(ALICE));
    list.unfreeze_address(&mut cap, ALICE, scenario.ctx());
    assert!(!list.is_frozen<TEST>(ALICE));
    teardown(scenario, list, cap);
}

#[test]
fun migrate_carries_over_pending_pause() {
    let (mut scenario, mut cap) = setup_deny_list(true);
    let mut deny_list = scenario.take_shared<DenyList>();
    coin::deny_list_v2_enable_global_pause(&mut deny_list, &mut cap, scenario.ctx());
    test_scenario::return_shared(deny_list);

    migrate(&mut scenario, cap, vector[]);
    scenario.next_tx(ISSUER);

    let list = scenario.take_shared<FreezeList>();
    assert!(list.is_global_pause_enabled<TEST>());
    let cap = scenario.take_from_sender<ComplianceCap<TEST>>();
    teardown(scenario, list, cap);
}

#[test]
fun migrate_carries_over_pending_unpause() {
    let (mut scenario, mut cap) = setup_deny_list(true);
    let mut deny_list = scenario.take_shared<DenyList>();
    coin::deny_list_v2_enable_global_pause(&mut deny_list, &mut cap, scenario.ctx());
    test_scenario::return_shared(deny_list);
    scenario.next_epoch(ISSUER);

    let mut deny_list = scenario.take_shared<DenyList>();
    coin::deny_list_v2_disable_global_pause(&mut deny_list, &mut cap, scenario.ctx());
    assert!(
        coin::deny_list_v2_is_global_pause_enabled_current_epoch<TEST>(
            &deny_list,
            scenario.ctx(),
        ),
    );
    test_scenario::return_shared(deny_list);

    migrate(&mut scenario, cap, vector[]);
    scenario.next_tx(ISSUER);

    let list = scenario.take_shared<FreezeList>();
    assert!(!list.is_global_pause_enabled<TEST>());
    let cap = scenario.take_from_sender<ComplianceCap<TEST>>();
    teardown(scenario, list, cap);
}

#[test]
fun migrate_leaves_other_types_on_deny_list() {
    let (mut scenario, cap) = setup_deny_list(false);
    migrate(&mut scenario, cap, vector[]);
    scenario.next_tx(ISSUER);

    let mut deny_list = scenario.take_shared<DenyList>();
    let mut other_cap = coin::new_deny_cap_v2<OTHER>(false, scenario.ctx());
    coin::deny_list_v2_add(&mut deny_list, &mut other_cap, ALICE, scenario.ctx());
    assert!(coin::deny_list_v2_contains_next_epoch<OTHER>(&deny_list, ALICE));
    destroy(other_cap);
    test_scenario::return_shared(deny_list);
    scenario.end();
}

#[test, expected_failure(abort_code = coin_regulation::ECoinTypeStateExists)]
fun migrate_second_deny_cap_aborts() {
    let (mut scenario, cap) = setup_deny_list(false);
    migrate(&mut scenario, cap, vector[]);
    scenario.next_tx(ISSUER);
    let cap = coin::new_deny_cap_v2<TEST>(false, scenario.ctx());
    migrate(&mut scenario, cap, vector[]);
    abort
}

#[test, expected_failure(abort_code = coin_regulation::ECoinTypeStateExists)]
fun migrate_after_freeze_aborts() {
    let (mut scenario, mut list, mut cap) = setup(false);
    list.freeze_address(&mut cap, ALICE, scenario.ctx());
    let mut deny_list = scenario.take_shared<DenyList>();
    let deny_cap = coin::new_deny_cap_v2<TEST>(false, scenario.ctx());
    let _cap = list.migrate_from_deny_list(&mut deny_list, deny_cap, vector[], scenario.ctx());
    abort
}

/// Migrate `TEST` and return the deny list with a fresh `DenyCapV2<TEST>`.
fun migrated_deny_list(): (Scenario, DenyList, DenyCapV2<TEST>) {
    let (mut scenario, cap) = setup_deny_list(true);
    migrate(&mut scenario, cap, vector[]);
    scenario.next_tx(ISSUER);
    let deny_list = scenario.take_shared<DenyList>();
    let cap = coin::new_deny_cap_v2<TEST>(true, scenario.ctx());
    (scenario, deny_list, cap)
}

#[test, expected_failure(abort_code = coin::ECoinTypeMigrated)]
fun legacy_add_aborts_after_migration() {
    let (mut scenario, mut deny_list, mut cap) = migrated_deny_list();
    coin::deny_list_v2_add(&mut deny_list, &mut cap, ALICE, scenario.ctx());
    abort
}

#[test, expected_failure(abort_code = coin::ECoinTypeMigrated)]
fun legacy_remove_aborts_after_migration() {
    let (mut scenario, mut deny_list, mut cap) = migrated_deny_list();
    coin::deny_list_v2_remove(&mut deny_list, &mut cap, ALICE, scenario.ctx());
    abort
}

#[test, expected_failure(abort_code = coin::ECoinTypeMigrated)]
fun legacy_contains_current_epoch_aborts_after_migration() {
    let (mut scenario, deny_list, _cap) = migrated_deny_list();
    coin::deny_list_v2_contains_current_epoch<TEST>(&deny_list, ALICE, scenario.ctx());
    abort
}

#[test, expected_failure(abort_code = coin::ECoinTypeMigrated)]
fun legacy_contains_next_epoch_aborts_after_migration() {
    let (_scenario, deny_list, _cap) = migrated_deny_list();
    coin::deny_list_v2_contains_next_epoch<TEST>(&deny_list, ALICE);
    abort
}

#[test, expected_failure(abort_code = coin::ECoinTypeMigrated)]
fun legacy_enable_global_pause_aborts_after_migration() {
    let (mut scenario, mut deny_list, mut cap) = migrated_deny_list();
    coin::deny_list_v2_enable_global_pause(&mut deny_list, &mut cap, scenario.ctx());
    abort
}

#[test, expected_failure(abort_code = coin::ECoinTypeMigrated)]
fun legacy_disable_global_pause_aborts_after_migration() {
    let (mut scenario, mut deny_list, mut cap) = migrated_deny_list();
    coin::deny_list_v2_disable_global_pause(&mut deny_list, &mut cap, scenario.ctx());
    abort
}

#[test, expected_failure(abort_code = coin::ECoinTypeMigrated)]
fun legacy_is_global_pause_enabled_current_epoch_aborts_after_migration() {
    let (mut scenario, deny_list, _cap) = migrated_deny_list();
    coin::deny_list_v2_is_global_pause_enabled_current_epoch<TEST>(&deny_list, scenario.ctx());
    abort
}

#[test, expected_failure(abort_code = coin::ECoinTypeMigrated)]
fun legacy_is_global_pause_enabled_next_epoch_aborts_after_migration() {
    let (_scenario, deny_list, _cap) = migrated_deny_list();
    coin::deny_list_v2_is_global_pause_enabled_next_epoch<TEST>(&deny_list);
    abort
}
