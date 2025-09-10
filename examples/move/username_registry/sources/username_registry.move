// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A module that demonstrates derived objects using Sui's derived_object framework.
/// It creates a shared registry where anyone can create deterministic derived username objects.
module username_registry::username_registry {
    use std::string::{Self, String};
    use sui::derived_object;

    /// Error codes
    const EUsernameEmpty: u64 = 0;
    const EUsernameTooLong: u64 = 1;
    const ENotOwner: u64 = 2;

    /// Maximum username length
    const MAX_USERNAME_LENGTH: u64 = 50;

    /// Shared registry object that anyone can use to create derived user objects
    public struct UsernameRegistry has key {
        id: UID,
        /// Counter for tracking total users created
        total_users: u64,
    }

    /// User profile object with username - derived from the registry using derived_object
    /// This is owned by the user who created it and has a deterministic address
    public struct UserProfile has key, store {
        id: UID,
        /// The username for this user
        username: String,
        /// The address of the user who owns this profile
        owner: address,
        /// Registry ID this was derived from
        registry_id: ID,
    }

    /// One-time witness for module initialization
    public struct USERNAME_REGISTRY has drop {}

    /// Initialize the module by creating a shared registry
    fun init(_witness: USERNAME_REGISTRY, ctx: &mut TxContext) {
        let registry = UsernameRegistry {
            id: object::new(ctx),
            total_users: 0,
        };
        
        // Share the registry so anyone can use it
        transfer::share_object(registry);
    }

    /// Create a new user profile derived from the shared registry using the sender's address as the key
    /// This ensures each address can only have one profile per registry
    /// The derived object will have a deterministic address based on the registry UID and sender address
    /// Returns the created profile for the caller to handle in their programmable transaction block
    public fun create_user_profile(
        registry: &mut UsernameRegistry,
        username: String,
        ctx: &mut TxContext
    ): UserProfile {
        // Validate username
        assert!(!string::is_empty(&username), EUsernameEmpty);
        assert!(string::length(&username) <= MAX_USERNAME_LENGTH, EUsernameTooLong);

        let sender = ctx.sender();
        
        // Claim a derived UID using the sender's address as the key
        // This ensures deterministic addresses and prevents duplicate profiles per address
        let derived_id = derived_object::claim(&mut registry.id, sender);

        // Increment user counter
        registry.total_users = registry.total_users + 1;

        // Create the derived user profile object
        let user_profile = UserProfile {
            id: derived_id,
            username,
            owner: sender,
            registry_id: object::id(registry),
        };

        // Return the profile for the caller to handle in their programmable transaction block
        user_profile
    }

    /// Set/update the username of a user profile
    /// Only the owner can update their username
    public fun set_username(
        profile: &mut UserProfile,
        new_username: String,
        ctx: &TxContext
    ) {
        // Only the owner can update their username
        assert!(profile.owner == ctx.sender(), ENotOwner);
        
        // Validate new username
        assert!(!string::is_empty(&new_username), EUsernameEmpty);
        assert!(string::length(&new_username) <= MAX_USERNAME_LENGTH, EUsernameTooLong);

        profile.username = new_username;
    }

    /// Get the username in human-readable string format
    public fun get_username(profile: &UserProfile): String {
        profile.username
    }

    /// Get the owner address of a user profile
    public fun get_owner(profile: &UserProfile): address {
        profile.owner
    }

    /// Get the registry ID this profile was derived from
    public fun get_registry_id(profile: &UserProfile): ID {
        profile.registry_id
    }

    /// Get the total number of users created from this registry
    public fun get_total_users(registry: &UsernameRegistry): u64 {
        registry.total_users
    }

    /// Get the registry's ID
    public fun get_registry_id_from_registry(registry: &UsernameRegistry): ID {
        object::id(registry)
    }

    /// Check if a user profile already exists for a given address in this registry
    /// This uses the derived_object exists function to check deterministically
    public fun profile_exists_for_address(registry: &UsernameRegistry, user_address: address): bool {
        derived_object::exists(&registry.id, user_address)
    }

    /// Calculate the deterministic address where a user's profile would be created
    /// This allows off-chain systems to predict addresses before creation
    public fun derive_profile_address(registry: &UsernameRegistry, user_address: address): address {
        derived_object::derive_address(object::id(registry), user_address)
    }

    #[test_only]
    /// Test-only function to create registry for testing
    public fun create_registry_for_testing(ctx: &mut TxContext): UsernameRegistry {
        UsernameRegistry {
            id: object::new(ctx),
            total_users: 0,
        }
    }

    #[test_only]
    /// Test-only function to create a user profile without transferring it
    public fun create_user_profile_for_testing(
        registry: &mut UsernameRegistry,
        username: String,
        ctx: &mut TxContext
    ): UserProfile {
        // Validate username
        assert!(!string::is_empty(&username), EUsernameEmpty);
        assert!(string::length(&username) <= MAX_USERNAME_LENGTH, EUsernameTooLong);

        let sender = ctx.sender();
        
        // Claim a derived UID using the sender's address as the key
        let derived_id = derived_object::claim(&mut registry.id, sender);

        // Increment user counter
        registry.total_users = registry.total_users + 1;

        // Create the derived user profile object
        UserProfile {
            id: derived_id,
            username,
            owner: sender,
            registry_id: object::id(registry),
        }
    }
}

#[test_only]
module username_registry::username_registry_tests {
    use username_registry::username_registry::{Self, UsernameRegistry, UserProfile};
    use std::string;
    use sui::test_scenario::{Self, Scenario};
    use sui::test_utils;

    const ALICE: address = @0xA11CE;
    const BOB: address = @0xB0B;

    #[test]
    fun test_create_user_profile() {
        let mut scenario = test_scenario::begin(ALICE);

        // Create registry
        let mut registry = username_registry::create_registry_for_testing(scenario.ctx());
        
        // Verify profile doesn't exist yet
        assert!(!username_registry::profile_exists_for_address(&registry, ALICE));
        
        // Create user profile
        let profile = username_registry::create_user_profile_for_testing(
            &mut registry,
            string::utf8(b"alice_user"),
            scenario.ctx()
        );

        // Verify profile data
        assert!(username_registry::get_username(&profile) == string::utf8(b"alice_user"));
        assert!(username_registry::get_owner(&profile) == ALICE);

        // Verify registry state
        assert!(username_registry::get_total_users(&registry) == 1);
        assert!(username_registry::profile_exists_for_address(&registry, ALICE));

        // Clean up
        test_utils::destroy(profile);
        test_utils::destroy(registry);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_set_username() {
        let mut scenario = test_scenario::begin(ALICE);

        // Create registry and profile
        let mut registry = username_registry::create_registry_for_testing(scenario.ctx());
        let mut profile = username_registry::create_user_profile_for_testing(
            &mut registry,
            string::utf8(b"alice_user"),
            scenario.ctx()
        );

        // Update username
        username_registry::set_username(
            &mut profile,
            string::utf8(b"alice_updated"),
            scenario.ctx()
        );

        // Verify update
        assert!(username_registry::get_username(&profile) == string::utf8(b"alice_updated"));

        // Clean up
        test_utils::destroy(profile);
        test_utils::destroy(registry);
        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code = username_registry::ENotOwner)]
    fun test_set_username_wrong_owner() {
        let mut scenario = test_scenario::begin(ALICE);

        // Create registry and profile as Alice
        let mut registry = username_registry::create_registry_for_testing(scenario.ctx());
        let mut profile = username_registry::create_user_profile_for_testing(
            &mut registry,
            string::utf8(b"alice_user"),
            scenario.ctx()
        );

        // Switch to Bob and try to update Alice's username (should fail)
        test_scenario::next_tx(&mut scenario, BOB);
        username_registry::set_username(
            &mut profile,
            string::utf8(b"bob_hacker"),
            scenario.ctx()
        );

        // Clean up
        test_utils::destroy(profile);
        test_utils::destroy(registry);
        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code = username_registry::EUsernameEmpty)]
    fun test_empty_username() {
        let mut scenario = test_scenario::begin(ALICE);

        let mut registry = username_registry::create_registry_for_testing(scenario.ctx());
        
        // Try to create profile with empty username (should fail)
        let profile = username_registry::create_user_profile_for_testing(
            &mut registry,
            string::utf8(b""),
            scenario.ctx()
        );

        // Clean up
        test_utils::destroy(profile);
        test_utils::destroy(registry);
        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code = sui::derived_object::EObjectAlreadyExists)]
    fun test_duplicate_profile_creation() {
        let mut scenario = test_scenario::begin(ALICE);

        // Create registry
        let mut registry = username_registry::create_registry_for_testing(scenario.ctx());
        
        // Alice creates first profile
        let profile1 = username_registry::create_user_profile_for_testing(
            &mut registry,
            string::utf8(b"alice_user"),
            scenario.ctx()
        );

        // Try to create another profile for Alice (should fail due to derived object already exists)
        let profile2 = username_registry::create_user_profile_for_testing(
            &mut registry,
            string::utf8(b"alice_user2"),
            scenario.ctx()
        );

        // Clean up
        test_utils::destroy(profile1);
        test_utils::destroy(profile2);
        test_utils::destroy(registry);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_multiple_users_different_addresses() {
        let mut scenario = test_scenario::begin(ALICE);

        // Create registry
        let mut registry = username_registry::create_registry_for_testing(scenario.ctx());
        
        // Alice creates profile
        let alice_profile = username_registry::create_user_profile_for_testing(
            &mut registry,
            string::utf8(b"alice_user"),
            scenario.ctx()
        );

        // Switch to Bob
        test_scenario::next_tx(&mut scenario, BOB);
        
        // Bob creates profile
        let bob_profile = username_registry::create_user_profile_for_testing(
            &mut registry,
            string::utf8(b"bob_user"),
            scenario.ctx()
        );

        // Verify both profiles
        assert!(username_registry::get_username(&alice_profile) == string::utf8(b"alice_user"));
        assert!(username_registry::get_owner(&alice_profile) == ALICE);

        assert!(username_registry::get_username(&bob_profile) == string::utf8(b"bob_user"));
        assert!(username_registry::get_owner(&bob_profile) == BOB);

        // Verify registry state
        assert!(username_registry::get_total_users(&registry) == 2);
        assert!(username_registry::profile_exists_for_address(&registry, ALICE));
        assert!(username_registry::profile_exists_for_address(&registry, BOB));

        // Both profiles should reference the same registry
        assert!(username_registry::get_registry_id(&alice_profile) == 
                username_registry::get_registry_id(&bob_profile));

        // Clean up
        test_utils::destroy(alice_profile);
        test_utils::destroy(bob_profile);
        test_utils::destroy(registry);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_derive_profile_address() {
        let mut scenario = test_scenario::begin(ALICE);

        // Create registry
        let mut registry = username_registry::create_registry_for_testing(scenario.ctx());
        
        // Get the predicted address for Alice's profile
        let predicted_address = username_registry::derive_profile_address(&registry, ALICE);
        
        // Create Alice's profile
        let profile = username_registry::create_user_profile_for_testing(
            &mut registry,
            string::utf8(b"alice_user"),
            scenario.ctx()
        );

        // Verify the actual address matches the predicted address
        assert!(object::id_address(&profile) == predicted_address);

        // Clean up
        test_utils::destroy(profile);
        test_utils::destroy(registry);
        test_scenario::end(scenario);
    }
}