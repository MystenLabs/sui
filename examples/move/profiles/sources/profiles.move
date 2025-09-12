// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A module that demonstrates derived objects using Sui's derived_object framework.
/// It creates a shared registry where anyone can create deterministic derived profile objects.
module profiles::profiles;

use std::string::String;
use sui::derived_object;
use sui::transfer::Receiving;

/// Error codes
#[error(code = 0)]
const EUsernameEmpty: vector<u8> = b"Username cannot be empty";
#[error(code = 1)]
const EUsernameTooLong: vector<u8> = b"Username cannot be longer than 50 characters";
#[error(code = 2)]
const ENotOwner: vector<u8> = b"Only the owner can update their username";

/// Maximum username length
const MAX_USERNAME_LENGTH: u64 = 50;

/// Shared registry object that anyone can use to create derived user objects
public struct ProfilesRegistry has key {
    id: UID,
    /// Counter for tracking total users created
    total_users: u64,
}

/// User profile object with username - derived from the registry using derived_object
/// This is owned by the user who created it and has a deterministic address
public struct Profile has key {
    id: UID,
    /// The username for this user
    username: String,
    /// The address of the user who owns this profile
    owner: address,
}

/// Initialize the module by creating a shared registry
fun init(ctx: &mut TxContext) {
    let registry = ProfilesRegistry {
        id: object::new(ctx),
        total_users: 0,
    };

    // Share the registry so anyone can use it to create their profiles.
    transfer::share_object(registry);
}

/// Create a new user profile derived from the shared registry using the sender's address as the key
/// This ensures each address can only have one profile per registry
/// The derived object will have a deterministic address based on the registry UID and sender address
/// Returns the created profile for the caller to handle in their programmable transaction block
public fun new_profile(
    registry: &mut ProfilesRegistry,
    username: String,
    ctx: &mut TxContext,
): Profile {
    validate_username!(username);

    let sender = ctx.sender();

    // Claim a derived UID using the sender's address as the key
    // This ensures deterministic addresses and prevents duplicate profiles per address.
    // You can now always figure out the Object ID of the profile, based on the user's
    // address & the registry's ID.
    let derived_id = derived_object::claim(&mut registry.id, sender);

    // Increment user counter
    registry.total_users = registry.total_users + 1;

    // Create the derived user profile object
    let user_profile = Profile {
        id: derived_id,
        username,
        owner: sender,
    };

    // Return the profile for the caller to handle in their programmable transaction block
    user_profile
}

/// Share the profile to make it accessible by anyone!
public fun share(profile: Profile) {
    transfer::share_object(profile);
}

/// Set/update the username of a user profile
/// Only the owner can update their username
public fun set_username(profile: &mut Profile, new_username: String, ctx: &TxContext) {
    // Only the owner can update their username
    assert!(profile.owner == ctx.sender(), ENotOwner);

    // Validate new username
    validate_username!(new_username);

    profile.username = new_username;
}

/// Get the username in human-readable string format
public fun username(profile: &Profile): String {
    profile.username
}

/// Get the owner address of a user profile
public fun owner(profile: &Profile): address {
    profile.owner
}

/// Get the total number of users created from this registry
public fun total_users(registry: &ProfilesRegistry): u64 {
    registry.total_users
}

/// Check if a user profile already exists for a given address in this registry
/// This uses the derived_object exists function to check deterministically
public fun profile_exists(registry: &ProfilesRegistry, user_address: address): bool {
    derived_object::exists(&registry.id, user_address)
}

/// Receive items transferred to the `Profile` object as the owner.
public fun receive<T: key + store>(
    profile: &mut Profile,
    object: Receiving<T>,
    ctx: &TxContext,
): T {
    assert!(profile.owner == ctx.sender(), ENotOwner);
    transfer::public_receive(&mut profile.id, object)
}

macro fun validate_username($name: String) {
    let name = $name;
    assert!(!name.is_empty(), EUsernameEmpty);
    assert!(name.length() <= MAX_USERNAME_LENGTH, EUsernameTooLong);
}

#[test_only]
/// Test-only function to create registry for testing
public fun create_registry_for_testing(ctx: &mut TxContext): ProfilesRegistry {
    ProfilesRegistry {
        id: object::new(ctx),
        total_users: 0,
    }
}
