// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Enables the creation of objects with deterministic addresses derived from a parent object's UID.
/// This module provides a way to generate objects with predictable addresses based on a parent UID
/// and a key, creating a namespace that ensures uniqueness for each parent-key combination,
/// which is usually how registries are built.
///
/// Key features:
/// - Deterministic address generation based on parent object UID and key
/// - Derived objects can exist and operate independently of their parent
///
/// The derived UIDs, once created, are independent and do not require sequencing on the parent
/// object. They can be used without affecting the parent. The parent only maintains a record of
/// which derived addresses have been claimed to prevent duplicates. Releasing a derived UID removes
/// its record, so the same address can be claimed again.
module sui::derived_object;

use sui::dynamic_field as df;

/// Tries to create an object twice with the same parent-key combination.
#[error(code = 0)]
const EObjectAlreadyExists: vector<u8> = b"Derived object is already claimed.";

/// Tries to release a UID that is not claimed under the given parent.
#[error(code = 1)]
const EObjectNotClaimed: vector<u8> = b"Derived object is not claimed by this parent.";

/// Tries to release the UID of a shared object.
#[error(code = 2)]
const ESharedObjectRelease: vector<u8> = b"Shared objects cannot be released.";

/// Added as a DF to the parent's UID, to mark an ID as claimed.
public struct Claimed(ID) has copy, drop, store;

/// An internal key to protect from generating the same UID twice (e.g. collide with DFs)
public struct DerivedObjectKey<K: copy + drop + store>(K) has copy, drop, store;

/// The possible values of a claimed UID.
/// We make it an enum to make upgradeability easier in the future.
public enum ClaimedStatus has store {
    /// The UID has been claimed. It may be in use or deleted with `object::delete`.
    /// `release` removes the marker.
    Reserved,
}

/// Claim a deterministic UID, using the parent's UID & any key.
public fun claim<K: copy + drop + store>(parent: &mut UID, key: K): UID {
    let addr = derive_address(parent.to_inner(), key);
    let id = addr.to_id();
    assert!(!df::exists(parent, Claimed(id)), EObjectAlreadyExists);
    df::add(parent, Claimed(id), ClaimedStatus::Reserved);
    object::new_uid_from_hash(parent.to_address(), addr)
}

/// Deletes `id`, claimed under `parent`, so that its address can be claimed again.
/// Dynamic fields of `id`, objects sent to its address, and its address balances persist and are
/// reachable again once it is re-claimed.
public fun release(parent: &mut UID, id: UID) {
    assert!(!id.is_shared_input(), ESharedObjectRelease);
    let claimed = Claimed(id.to_inner());
    assert!(df::exists(parent, claimed), EObjectNotClaimed);
    let status: ClaimedStatus = df::remove(parent, claimed);
    match (status) {
        ClaimedStatus::Reserved => (),
    };
    id.delete()
}

/// Checks if a provided `key` has been claimed for the given parent.
/// Note: If the UID has been deleted through `object::delete` rather than `release`, this will
/// always return true.
public fun exists<K: copy + drop + store>(parent: &UID, key: K): bool {
    let addr = derive_address(parent.to_inner(), key);
    df::exists(parent, Claimed(addr.to_id()))
}

/// Given an ID and a Key, it calculates the derived address.
public fun derive_address<K: copy + drop + store>(parent: ID, key: K): address {
    df::hash_type_and_key(parent.to_address(), DerivedObjectKey(key))
}
