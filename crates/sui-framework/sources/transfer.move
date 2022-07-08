// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::transfer {
    use sui::id::{Self, TransferredID};

    /// Transfers are implemented by emitting a
    /// special `TransferEvent` that the sui adapter
    /// interprets differently than user events.
    struct TransferEvent<T: key> {
        /// The object to be transferred
        obj: T,
        /// Address the object will be transferred to.
        recipient: address,
    }

    /// Transfer ownership of `obj` to `recipient`. `obj` must have the
    /// `key` attribute, which (in turn) ensures that `obj` has a globally
    /// unique ID.
    public fun transfer<T: key>(obj: T, recipient: address): TransferredID {
        // TODO: emit event
        let id = *id::id(&obj);
        transfer_internal(obj, recipient, false);
        id::new_transferred_id(id)
    }

    /// Transfer ownership of `obj` to another object `owner`.
    public fun transfer_to_object<T: key, R: key>(obj: T, owner: &mut R): TransferredID {
        let obj_id = *id::id(&obj);
        let owner_id = id::id_address(id::id(owner));
        transfer_internal(obj, owner_id, true);
        id::new_transferred_id(obj_id)
    }

    /// Transfer ownership of `obj` to another object whose ID is `owner_id`.
    public fun transfer_to_object_id<T: key>(obj: T, owner_id: TransferredID): TransferredID {
        let obj_id = *id::id(&obj);
        let inner_owner_id = *id::transferred_inner(&owner_id);
        transfer_internal(obj, id::id_address(&inner_owner_id), true);
        id::new_transferred_id(obj_id)
    }

    /// Freeze `obj`. After freezing `obj` becomes immutable and can no
    /// longer be transferred or mutated.
    public fun freeze_object<T: key>(obj: T): TransferredID {
        let id = *id::id(&obj);
        freeze_object_internal(obj);
        id::new_transferred_id(id)
    }
    native fun freeze_object_internal<T: key>(obj: T);

    /// Turn the given object into a mutable shared object that everyone
    /// can access and mutate. This is irreversible, i.e. once an object
    /// is shared, it will stay shared forever.
    /// Shared mutable object is not yet fully supported in Sui, which is being
    /// actively worked on and should be supported very soon.
    /// https://github.com/MystenLabs/sui/issues/633
    /// https://github.com/MystenLabs/sui/issues/681
    /// This API is exposed to demonstrate how we may be able to use it to program
    /// Move contracts that use shared objects.
    public fun share_object<T: key>(obj: T): TransferredID {
        let id = *id::id(&obj);
        share_object_internal(obj);
        id::new_transferred_id(id)
    }
    native fun share_object_internal<T: key>(obj: T);

    native fun transfer_internal<T: key>(obj: T, recipient: address, to_object: bool);

    // Cost calibration functions
    #[test_only]
    public fun calibrate_freeze_object<T: key>(obj: T) {
        freeze_object_internal(obj)
    }
    #[test_only]
    public fun calibrate_freeze_object_nop<T: key + drop>(_obj: T) {
    }

    #[test_only]
    public fun calibrate_share_object<T: key>(obj: T) {
        share_object_internal(obj)
    }
    #[test_only]
    public fun calibrate_share_object_nop<T: key + drop>(_obj: T) {
    }

    #[test_only]
    public fun calibrate_transfer_internal<T: key>(obj: T, recipient: address, to_object: bool) {
        transfer_internal(obj, recipient, to_object)
    }
    #[test_only]
    public fun calibrate_transfer_internal_nop<T: key + drop>(_obj: T, _recipient: address, _to_object: bool) {
    }

    // TBD
    // #[test_only]
    // public fun calibrate_delete_child_object_internal_nop(_child: address, _child_id: VersionedID) {
    // }

}
