// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::transfer {
    use sui::object::{Self, UID};

    /// Transfer ownership of `obj` to `recipient`. `obj` must have the
    /// `key` attribute, which (in turn) ensures that `obj` has a globally
    /// unique ID.
    public fun transfer<T: key>(obj: T, recipient: address) {
        // TODO: emit event
        transfer_internal(obj, recipient, false)
    }

    /// Transfer ownership of `obj` to another object `owner`.
    public fun transfer_to_object<T: key, R: key>(obj: T, owner: &mut R) {
        let owner_id = object::id_address(owner);
        transfer_internal(obj, owner_id, true);
    }

    /// Similar to transfer_to_object where we want to transfer an object to another object.
    /// However, in the case when we haven't yet created the parent object (typically during
    /// parent object construction), and all we have is just a parent object ID, we could
    /// use this function to transfer an object to the parent object identified by its id.
    /// Additionally, this API is useful for transfering to objects, outside of that object's
    /// module. The object's module can expose a function that returns a reference to the object's
    /// UID, `&mut UID`, which can then be used with this function. The mutable `&mut UID` reference
    /// prevents child objects from being added to immutable objects (immutable objects cannot have
    /// child objects).
    /// The child object is specified in `obj`, and the parent object id is specified in `owner_id`.
    public fun transfer_to_object_id<T: key>(obj: T, owner_id: &mut UID) {
        transfer_internal(obj, object::uid_to_address(owner_id), true);
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

    // Cost calibration functions
    #[test_only]
    public fun calibrate_freeze_object<T: key>(obj: T) {
        freeze_object(obj);
    }
    #[test_only]
    public fun calibrate_freeze_object_nop<T: key + drop>(obj: T) {
        let _ = obj;
    }

    #[test_only]
    public fun calibrate_share_object<T: key>(obj: T) {
        share_object(obj);
    }
    #[test_only]
    public fun calibrate_share_object_nop<T: key + drop>(obj: T) {
        let _ = obj;
    }

    #[test_only]
    public fun calibrate_transfer_internal<T: key>(obj: T, recipient: address, to_object: bool) {
        transfer_internal(obj, recipient, to_object);
    }
    #[test_only]
    public fun calibrate_transfer_internal_nop<T: key + drop>(obj: T, recipient: address, to_object: bool) {
        let _ = obj;
        let _ = recipient;
        let _ = to_object;
    }

}
