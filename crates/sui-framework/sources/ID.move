// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Sui object identifiers
module Sui::ID {
    use std::bcs;
    use std::vector;

    friend Sui::SuiSystem;
    friend Sui::Transfer;
    friend Sui::TxContext;

    #[test_only]
    friend Sui::TestScenario;

    /// Version of an object ID created by the current transaction.
    const INITIAL_VERSION: u64 = 0;

    /// The hardcoded ID for the singleton Sui System State Object.
    const SUI_SYSTEM_STATE_OBJECT_ID: address = @0x5;

    /// Number of bytes in an object ID
    const ID_SIZE: u64 = 20;

    /// Attempting to construct an object ID with the wrong number of bytes--expected 20.
    const EBadIDLength: u64 = 0;

    /// Globally unique identifier of an object. This is a privileged type
    /// that can only be derived from a `TxContext`.
    // Currently, this is not exposed and is thus somewhat redundant, but
    // in the future we may want a type representing a globally unique ID
    // without a version.
    struct UniqueID has store {
        id: ID
    }

    /// An object ID. Unlike `UniqueID`, this is *not* guaranteed to be globally
    /// unique--anyone can create an `ID`, and ID's can be freely copied and dropped
    /// Useful for comparing with `UniqueID`'s.
    struct ID has store, drop, copy {
        // We use `address` instead of `vector<u8>` here because `address` has a more
        // compact serialization. `address` is serialized as a BCS fixed-length sequence,
        // which saves us the length prefix we would pay for if this were `vector<u8>`.
        // See https://github.com/diem/bcs#fixed-and-variable-length-sequences.
        bytes: address
    }

    /// A globally unique ID paired with a version. Similar to `UniqueID`,
    /// this is a privileged type that can only be derived from a `TxContext`
    /// VersionedID doesn't have the `drop` ability, so deleting a `VersionedID`
    /// requires a call to `ID::delete`
    struct VersionedID has store {
        id: UniqueID,
        /// Version number for the object. The version number is incremented each
        /// time the object with this VersionedID is passed to a non-failing transaction
        /// either by value or by mutable reference.
        /// Note: if the object with this VersionedID gets wrapped in another object, the
        /// child object may be mutated with no version number change.
        version: u64
    }

    // === constructors ===

    /// Create an `ID` from an address
    public fun new(a: address): ID {
        ID { bytes: a }
    }

    /// Create an `ID` from raw bytes.
    /// Aborts with `EBadIDLength` if the length of `bytes` is not `ID_SIZE`
    public fun new_from_bytes(bytes: vector<u8>): ID {
        if (vector::length(&bytes) != ID_SIZE) {
            abort(EBadIDLength)
        };
        ID { bytes: bytes_to_address(bytes) }
    }

    /// Create a new `VersionedID`. Only callable by `TxContext`.
    /// This is the only way to create either a `VersionedID` or a `UniqueID`.
    public(friend) fun new_versioned_id(bytes: address): VersionedID {
        VersionedID { id: UniqueID { id: ID { bytes } }, version: INITIAL_VERSION }
    }

    /// Create the `VersionedID` for the singleton SuiSystemState object.
    /// This should only be called once from SuiSsytem.
    public(friend) fun get_sui_system_state_object_id(): VersionedID {
        new_versioned_id(SUI_SYSTEM_STATE_OBJECT_ID)
    }

    // === reads ===

    /// Get the underlying `ID` of `obj`
    public fun id<T: key>(obj: &T): &ID {
        let versioned_id = get_versioned_id(obj);
        inner(versioned_id)
    }

    /// Get raw bytes for the underlying `ID` of `obj`
    public fun id_bytes<T: key>(obj: &T): vector<u8> {
        let versioned_id = get_versioned_id(obj);
        inner_bytes(versioned_id)
    }

    /// Get the raw bytes of `id`
    public fun bytes(id: &ID): vector<u8> {
        bcs::to_bytes(&id.bytes)
    }

    /// Get the inner `ID` of `versioned_id`
    public fun inner(versioned_id: &VersionedID): &ID {
        &versioned_id.id.id
    }

    /// Get the raw bytes of a `versioned_id`'s inner `ID`
    public fun inner_bytes(versioned_id: &VersionedID): vector<u8> {
        bytes(inner(versioned_id))
    }

    /// Get the inner bytes of `id` as an address.
    // Only used by `Transfer` and `TestSecnario`, but may expose in the future
    public(friend) fun id_address(id: &ID): address {
        id.bytes
    }

    /// Get the `version` of `obj`.
    // Private and unused for now, but may expose in the future
    fun version<T: key>(obj: &T): u64 {
        let versioned_id = get_versioned_id(obj);
        versioned_id.version
    }

    /// Return `true` if `obj` was created by the current transaction,
    /// `false` otherwise.
    // Private and unused for now, but may expose in the future
    fun created_by_current_tx<T: key>(obj: &T): bool {
        version(obj) == INITIAL_VERSION
    }

    /// Get the VersionedID for `obj`.
    // Safe because Sui has an extra
    // bytecode verifier pass that forces every struct with
    // the `key` ability to have a distinguished `VersionedID` field.
    // Private for now, but may expose in the future.
    native fun get_versioned_id<T: key>(obj: &T): &VersionedID;

    // === destructors ===

    /// Delete `id`. This is the only way to eliminate a `VersionedID`.
    // This exists to inform Sui of object deletions. When an object
    // gets unpacked, the programmer will have to do something with its
    // `VersionedID`. The implementation of this function emits a deleted
    // system event so Sui knows to process the object deletion
    public fun delete(versioned_id: VersionedID) {
        delete_id(versioned_id)
    }

    native fun delete_id<VersionedID>(id: VersionedID);

    // === internal functions ===

    /// Convert raw bytes into an address
    native fun bytes_to_address(bytes: vector<u8>): address;
}
