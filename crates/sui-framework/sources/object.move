// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Sui object identifiers
module sui::object {
    use std::bcs;
    use std::vector;
    // use std::option::{Self, Option};
    use sui::tx_context::{Self, TxContext};

    friend sui::sui_system;
    friend sui::transfer;

    #[test_only]
    friend sui::test_scenario;

    /// Version of an object ID created by the current transaction.
    const INITIAL_VERSION: u64 = 0;

    /// The hardcoded ID for the singleton Sui System State Object.
    const SUI_SYSTEM_STATE_OBJECT_ID: address = @0x5;

    /// Number of bytes in an object ID
    const ID_SIZE: u64 = 20;

    /// Attempting to construct an object ID with the wrong number of bytes--expected 20.
    const EBadIDLength: u64 = 0;

    /// Attempting to delete a parent object that still has children
    const EParentHasChildren: u64 = 0;

    /// An object ID. This is *not* guaranteed to be globally
    /// unique--anyone can create an `ID`, and ID's can be freely copied and dropped.
    struct ID has copy, drop, store {
        // We use `address` instead of `vector<u8>` here because `address` has a more
        // compact serialization. `address` is serialized as a BCS fixed-length sequence,
        // which saves us the length prefix we would pay for if this were `vector<u8>`.
        // See https://github.com/diem/bcs#fixed-and-variable-length-sequences.
        bytes: address
    }

    /// The information that defines an object. It contains
    /// - A globally unique ID
    /// - A version
    /// - The number of child objects
    /// This is a privileged type that can only be derived from a `TxContext`.
    /// `Info` doesn't have the `drop` ability, so deleting a `Info` requires a call to `delete`
    struct Info has store {
        id: ID,
        /// Version number for the object. The version number is incremented each
        /// time the object with this ID is passed to a non-failing transaction
        /// either by value or by mutable reference.
        version: u64,
        // /// The number of child objects. In order to delete the `Info`, this must be none or 0
        // child_count: Option<u64>,
    }

    // === id ===

    /// Create an `ID` from an address
    public fun id_from_address(a: address): ID {
        ID { bytes: a }
    }

    /// Create an `ID` from raw bytes.
    /// Aborts with `EBadIDLength` if the length of `bytes` is not `ID_SIZE`
    public fun id_from_bytes(bytes: vector<u8>): ID {
        assert!(vector::length(&bytes) == ID_SIZE, EBadIDLength);
        ID { bytes: bytes_to_address(bytes) }
    }

    /// Get the raw bytes of `id`
    public fun id_to_bytes(id: &ID): vector<u8> {
        bcs::to_bytes(&id.bytes)
    }

    // === info ===

    /// Create the `Info` for the singleton `SuiSystemState` object.
    /// This should only be called once from `sui_system`.
    public(friend) fun sui_system_state(): Info {
        Info {
            id: ID { bytes: SUI_SYSTEM_STATE_OBJECT_ID },
            version: INITIAL_VERSION,
            // child_count: option::none(),
        }
    }

    /// Get the inner `ID` of `versioned_id`
    public fun info_id(info: &Info): &ID {
        &info.id
    }

    /// Get the raw bytes of a `versioned_id`'s inner `ID`
    public fun info_id_bytes(info: &Info): vector<u8> {
        id_to_bytes(info_id(info))
    }

    // /// Get the number of child objects.
    // /// Returns 0 if `child_count` is none
    // public fun info_child_count(info: &Info): u64 {
    //     option::get_with_default(&info.child_count, 0)
    // }

    // === any object ===

    /// Create a new object. Returns the `Info` that must be stored in a Sui object.
    /// This is the only way to create `Info`.
    public fun new(ctx: &mut TxContext): Info {
        Info {
            id: ID { bytes: tx_context::new_object(ctx) },
            version: INITIAL_VERSION,
            // child_count: option::none(),
        }
    }

    /// Delete the object and it's `Info`. This is the only way to eliminate a `Info`.
    // This exists to inform Sui of object deletions. When an object
    // gets unpacked, the programmer will have to do something with its
    // `Info`. The implementation of this function emits a deleted
    // system event so Sui knows to process the object deletion
    public fun delete(info: Info) {
        // assert!(info_child_count(&info) == 0, EParentHasChildren);
        delete_impl(info)
    }

    /// Get the underlying `ID` of `obj`
    public fun id<T: key>(obj: &T): &ID {
        info_id(get_info(obj))
    }

    /// Get raw bytes for the underlying `ID` of `obj`
    public fun id_bytes<T: key>(obj: &T): vector<u8> {
        info_id_bytes(get_info(obj))
    }

    // /// Get the number of child objects.
    // public fun child_count<T: key>(obj: &T): u64 {
    //     info_child_count(get_info(obj))
    // }

    /// Get the `version` of `obj`.
    // Private and unused for now, but may expose in the future
    fun version<T: key>(obj: &T): u64 {
        let versioned_id = get_info(obj);
        versioned_id.version
    }

    /// Get the inner bytes of `id` as an address.
    // Only used by `Transfer` and `TestSecnario`, but may expose in the future
    public(friend) fun id_address(id: &ID): address {
        id.bytes
    }

    /// Return `true` if `obj` was created by the current transaction,
    /// `false` otherwise.
    // Private and unused for now, but may expose in the future
    fun created_by_current_tx<T: key>(obj: &T): bool {
        version(obj) == INITIAL_VERSION
    }

    /// Get the `Info` for `obj`.
    // Safe because Sui has an extra
    // bytecode verifier pass that forces every struct with
    // the `key` ability to have a distinguished `Info` field.
    native fun get_info<T: key>(obj: &T): &Info;

    // === destructors ===


    native fun delete_impl<Info>(info: Info);

    // === internal functions ===

    /// Convert raw bytes into an address
    native fun bytes_to_address(bytes: vector<u8>): address;

    // Cost calibration functions
    #[test_only]
    public fun calibrate_bytes_to_address(bytes: vector<u8>) {
        bytes_to_address(bytes);
    }
    #[test_only]
    public fun calibrate_bytes_to_address_nop(bytes: vector<u8>) {
        let _ = bytes;
    }

    #[test_only]
    public fun calibrate_get_info<T: key>(obj: &T) {
        get_info(obj);
    }
    #[test_only]
    public fun calibrate_get_info_nop<T: key>(obj: &T) {
        let _ = obj;
    }

    // TBD

    // #[test_only]
    // public fun calibrate_delete_impl(info: Info) {
    //     delete_impl(id);
    // }
    // #[test_only]
    // public fun calibrate_delete_impl(_id: Info) {
    // }

    #[test_only]
    /// Return the most recent created object ID.
    public fun last_created(ctx: &TxContext): ID {
        id_from_address(tx_context::last_created_object_id(ctx))
    }
}
