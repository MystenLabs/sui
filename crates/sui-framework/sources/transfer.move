// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::transfer {
    use std::option::{Self, Option};
    use sui::id::{Self, ID, VersionedID};

    // To allow access to transfer_to_object_unsafe.
    friend sui::bag;
    // To allow access to is_child_unsafe.
    friend sui::collection;

    // When transferring a child object, this error is thrown if the child object
    // doesn't match the ChildRef that represents the ownership.
    const EChildIDMismatch: u64 = 0;

    /// Represents a reference to a child object, whose type is T.
    /// This is used to track ownership between objects.
    /// Whenever an object is transferred to another object (and hence owned by object),
    /// a ChildRef is created. A ChildRef cannot be dropped. When a child object is
    /// transferred to a new parent object, the original ChildRef is dropped but a new
    /// one will be created. The only way to fully destroy a ChildRef is to transfer the
    /// object to an account address. Because of this, an object cannot be deleted when
    /// it's still owned by another object.
    struct ChildRef<phantom T: key> has store {
        child_id: ID,
    }

    /// Check whether the `child` object is actually the child object
    /// owned by the parent through the given `child_ref`.
    public fun is_child<T: key>(child_ref: &ChildRef<T>, child: &T): bool {
        &child_ref.child_id == id::id(child)
    }

    /// Check whether the `child_ref`'s child_id is `id`.
    /// This is less safe compared to `is_child` because we won't be able to check
    /// whether the type of the child object is the same as what `ChildRef` represents.
    /// We should always call `is_child` whenever we can.
    /// This is currently only exposed to friend classes. If there turns out to be
    /// general needs, we can open it up.
    public(friend) fun is_child_unsafe<T: key>(child_ref: &ChildRef<T>, id: &ID): bool {
        &child_ref.child_id == id
    }

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
    public fun transfer<T: key>(obj: T, recipient: address) {
        // TODO: emit event
        transfer_internal(obj, recipient, false)
    }

    /// Transfer ownership of `obj` to another object `owner`.
    /// Returns a non-droppable struct ChildRef that represents the ownership.
    public fun transfer_to_object<T: key, R: key>(obj: T, owner: &mut R): ChildRef<T> {
        let obj_id = *id::id(&obj);
        let owner_id = id::id_address(id::id(owner));
        transfer_internal(obj, owner_id, true);
        ChildRef { child_id: obj_id }
    }

    /// Similar to transfer_to_object where we want to transfer an object to another object.
    /// However, in the case when we haven't yet created the parent object (typically during
    /// parent object construction), and all we have is just a parent object ID, we could
    /// use this function to transfer an object to the parent object identified by its id.
    /// The child object is specified in `obj`, and the parent object id is specified in `owner_id`.
    /// The function consumes `owner_id` to make sure that the caller actually owns the id.
    /// The `owner_id` will be returned (so that it can be used to continue creating the parent object),
    /// along returned is the ChildRef as a reference to the ownership.
    public fun transfer_to_object_id<T: key>(obj: T, owner_id: VersionedID): (VersionedID, ChildRef<T>) {
        let obj_id = *id::id(&obj);
        let inner_owner_id = *id::inner(&owner_id);
        transfer_internal(obj, id::id_address(&inner_owner_id), true);
        let child_ref = ChildRef { child_id: obj_id };
        (owner_id, child_ref)
    }

    /// Similar to transfer_to_object, to transfer an object to another object.
    /// However it does not return the ChildRef. This can be unsafe to use since there is
    /// no longer guarantee that the ID stored in the parent actually represent ownership.
    /// If the object was owned by another object, an `old_child_ref` would be around
    /// and need to be consumed as well.
    public(friend) fun transfer_to_object_unsafe<T: key, R: key>(
        obj: T,
        old_child_ref: Option<ChildRef<T>>,
        owner: &mut R,
    ) {
        let ChildRef { child_id: _ } = if (option::is_none(&old_child_ref)) {
            transfer_to_object(obj, owner)
        } else {
            let child_ref = option::extract(&mut old_child_ref);
            transfer_child_to_object(obj, child_ref, owner)
        };
        option::destroy_none(old_child_ref);
    }

    /// Transfer a child object to new owner. This is one of the two ways that can
    /// consume a ChildRef. It will return a ChildRef that represents the new ownership.
    public fun transfer_child_to_object<T: key, R: key>(child: T, child_ref: ChildRef<T>, owner: &mut R): ChildRef<T> {
        let ChildRef { child_id } = child_ref;
        assert!(&child_id == id::id(&child), EChildIDMismatch);
        transfer_to_object(child, owner)
    }

    /// Transfer a child object to an account address. This is one of the two ways that can
    /// consume a ChildRef. No new ChildRef will be created, as the object is no longer
    /// owned by an object.
    public fun transfer_child_to_address<T: key>(child: T, child_ref: ChildRef<T>, recipient: address) {
        let ChildRef { child_id } = child_ref;
        assert!(&child_id == id::id(&child), EChildIDMismatch);
        transfer(child, recipient)
    }

    /// Delete `child_ref`, which must point at `child_id`.
    /// This is the second way to consume a `ChildRef`.
    /// Passing ownership of `child_id` to this function implies that the child object
    /// has been unpacked, so it is now safe to delete `child_ref`.
    public fun delete_child_object<T: key>(child_id: VersionedID, child_ref: ChildRef<T>) {
        let ChildRef { child_id: child_ref_id } = child_ref;
        assert!(&child_ref_id == id::inner(&child_id), EChildIDMismatch);
        delete_child_object_internal(id::id_address(&child_ref_id), child_id)
    }

    /// Freeze `obj`. After freezing `obj` becomes immutable and can no
    /// longer be transferred or mutated.
    public native fun freeze_object<T: key>(obj: T);

    /// Turn the given object into a mutable shared object that everyone
    /// can access and mutate. This is irreversible, i.e. once an object
    /// is shared, it will stay shared forever.
    /// Shared mutable object is not yet fully supported in Sui, which is being
    /// actively worked on and should be supported very soon.
    /// https://github.com/MystenLabs/sui/issues/633
    /// https://github.com/MystenLabs/sui/issues/681
    /// This API is exposed to demonstrate how we may be able to use it to program
    /// Move contracts that use shared objects.
    public native fun share_object<T: key>(obj: T);

    native fun transfer_internal<T: key>(obj: T, recipient: address, to_object: bool);

    // delete `child_id`, emit a system `DeleteChildObject(child)` event
    native fun delete_child_object_internal(child: address, child_id: VersionedID);

    // Cost calibration functions
    #[test_only]
    public fun calibrate_freeze_object<T: key>(obj: T) {
        freeze_object(obj)
    }
    #[test_only]
    public fun calibrate_freeze_object_nop<T: key + drop>(_obj: T) {
    }

    #[test_only]
    public fun calibrate_share_object<T: key>(obj: T) {
        share_object(obj)
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

    #[test_only]
    public fun calibrate_delete_child_object_internal(child: address, child_id: VersionedID) {
        delete_child_object_internal(child, child_id)
    }

    // TBD
    // #[test_only]
    // public fun calibrate_delete_child_object_internal_nop(_child: address, _child_id: VersionedID) {
    // }

}
