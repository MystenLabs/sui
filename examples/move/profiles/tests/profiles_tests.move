// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module profiles::profiles_tests;

use profiles::profiles;
use std::unit_test::destroy;
use sui::derived_object;
use sui::test_scenario;

const ALICE: address = @0xA11CE;
const BOB: address = @0xB0B;

#[test]
fun test_create_user_profile() {
    let mut scenario = test_scenario::begin(ALICE);

    // Create registry
    let mut registry = profiles::create_registry_for_testing(scenario.ctx());

    // Verify profile doesn't exist yet
    assert!(!registry.profile_exists(ALICE));

    // Create user profile
    let profile = registry.new_profile(b"alice_user".to_string(), scenario.ctx());

    // Verify profile data
    assert!(profile.username() == b"alice_user".to_string());
    assert!(profile.owner() == ALICE);

    // Verify registry state
    assert!(registry.total_users() == 1);
    assert!(registry.profile_exists(ALICE));

    // Clean up
    profile.share();
    destroy(registry);
    scenario.end();
}

#[test]
fun test_set_username() {
    let mut scenario = test_scenario::begin(ALICE);

    // Create registry and profile
    let mut registry = profiles::create_registry_for_testing(scenario.ctx());
    let mut profile = registry.new_profile(b"alice_user".to_string(), scenario.ctx());

    // Update username
    profile.set_username(b"alice_updated".to_string(), scenario.ctx());

    // Verify update
    assert!(profiles::username(&profile) == b"alice_updated".to_string());

    // Clean up
    profile.share();
    destroy(registry);
    scenario.end();
}

#[test]
#[expected_failure(abort_code = profiles::ENotOwner)]
fun test_set_username_wrong_owner() {
    let mut scenario = test_scenario::begin(ALICE);

    // Create registry and profile as Alice
    let mut registry = profiles::create_registry_for_testing(scenario.ctx());
    let mut profile = registry.new_profile(b"alice_user".to_string(), scenario.ctx());

    // Switch to Bob and try to update Alice's username (should fail)
    scenario.next_tx(BOB);
    profile.set_username(b"bob_hacker".to_string(), scenario.ctx());

    abort
}

#[test]
#[expected_failure(abort_code = profiles::EUsernameEmpty)]
fun test_empty_username() {
    let mut scenario = test_scenario::begin(ALICE);

    let mut registry = profiles::create_registry_for_testing(scenario.ctx());

    // Try to create profile with empty username (should fail)
    let _profile = registry.new_profile(b"".to_string(), scenario.ctx());

    abort
}

#[test]
#[expected_failure(abort_code = sui::derived_object::EObjectAlreadyExists)]
fun test_duplicate_profile_creation() {
    let mut scenario = test_scenario::begin(ALICE);

    // Create registry
    let mut registry = profiles::create_registry_for_testing(scenario.ctx());

    // Alice creates first profile
    let _profile1 = registry.new_profile(b"alice_user".to_string(), scenario.ctx());

    // Try to create another profile for Alice (should fail due to derived object already exists)
    let _profile2 = registry.new_profile(b"alice_user".to_string(), scenario.ctx());

    abort
}

#[test]
fun test_multiple_users_different_addresses() {
    let mut scenario = test_scenario::begin(ALICE);

    // Create registry
    let mut registry = profiles::create_registry_for_testing(scenario.ctx());

    // Alice creates profile
    let alice_profile = registry.new_profile(b"alice_user".to_string(), scenario.ctx());

    alice_profile.share();
    // Switch to Bob
    scenario.next_tx(BOB);

    let alice_profile_id = derived_object::derive_address(object::id(&registry), ALICE);

    // take Alice (with a known ID)
    let alice_profile = scenario.take_shared_by_id<profiles::Profile>(alice_profile_id.to_id());

    // Bob creates profile
    let bob_profile = registry.new_profile(b"bob_user".to_string(), scenario.ctx());

    // Verify both profiles
    assert!(alice_profile.username() == b"alice_user".to_string());
    assert!(alice_profile.owner() == ALICE);

    assert!(bob_profile.username() == b"bob_user".to_string());
    assert!(bob_profile.owner() == BOB);

    // Verify registry state
    assert!(registry.total_users() == 2);
    assert!(registry.profile_exists(ALICE));
    assert!(registry.profile_exists(BOB));

    test_scenario::return_shared(alice_profile);

    // Clean up
    bob_profile.share();
    destroy(registry);
    scenario.end();
}

#[test]
fun test_derive_profile_address() {
    let mut scenario = test_scenario::begin(ALICE);

    // Create registry
    let mut registry = profiles::create_registry_for_testing(scenario.ctx());

    // Get the predicted address for Alice's profile
    let predicted_address = derived_object::derive_address(object::id(&registry), ALICE);

    // Create Alice's profile
    let profile = registry.new_profile(b"alice_user".to_string(), scenario.ctx());

    // Verify the actual address matches the predicted address
    assert!(object::id_address(&profile) == predicted_address);

    // Clean up
    profile.share();
    destroy(registry);
    scenario.end();
}
