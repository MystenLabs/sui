// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::transfer {
    use sui::id::{Self, ID, VersionedID};

    // To allow access to transfer_to_object_unsafe.
    // friend sui::bag;
    // To allow access to is_child_unsafe.
    // friend sui::collection;

    // When transferring a child object, this error is thrown if the child object
    // doesn't match the ChildRef that represents the ownership.
    const EChildIDMismatch: u64 = 0;

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

    struct Imm<phantom T: key> has copy, drop { parent: ID, child: ID }
    struct Mut<phantom T: key> has drop { parent: ID, child: ID }
    struct Owned<phantom T: key> has drop { parent: ID, child: ID }

    /// Transfer ownership of `obj` to another object `owner`.
    /// Returns a non-droppable struct ChildRef that represents the ownership.
    public fun transfer_to_object<T: key, R: key>(obj: T, owner: &mut R) {
        let owner_id = id::id_address(id::id(owner));
        transfer_internal(obj, owner_id, true);
    }

    /// Similar to transfer_to_object where we want to transfer an object to another object.
    /// However, in the case when we haven't yet created the parent object (typically during
    /// parent object construction), and all we have is just a parent object ID, we could
    /// use this function to transfer an object to the parent object identified by its id.
    /// The child object is specified in `obj`, and the parent object id is specified in `owner_id`.
    /// The function consumes `owner_id` to make sure that the caller actually owns the id.
    /// The `owner_id` will be returned (so that it can be used to continue creating the parent object),
    /// along returned is the ChildRef as a reference to the ownership.
    public fun transfer_to_object_id<T: key>(obj: T, owner_id: VersionedID): VersionedID {
        let inner_owner_id = *id::inner(&owner_id);
        transfer_internal(obj, id::id_address(&inner_owner_id), true);
        owner_id
    }

    public fun freeze_child_ref<T: key>(mut_child_ref: Mut<T>): Imm<T> {
        let Mut { parent, child } = mut_child_ref;
        Imm { parent, child }
    }

    public fun borrow_child<Parent: key, Child: key>(parent: &VersionedID, child_ref: Imm<Child>): &Child {
        assert!(id::inner(parent) == &child_ref.parent, 0);
        let Imm { parent: _, child } = child_ref;
        borrow_child_internal(child)
    }

    public fun borrow_child_mut<Parent: key, Child: key>(parent: &mut VersionedID, child_ref: Mut<Child>): &Child {
        assert!(id::inner(parent) == &child_ref.parent, 0);
        let Mut { parent: _, child } = child_ref;
        borrow_child_mut_internal(child)
    }

    public fun take_child<Parent: key, Child: key>(parent: &mut VersionedID, child: Owned<Child>): Child {
        assert!(id::inner(parent) == &child.parent, 0);
        let Owned { parent: _, child } = child;
        take_child_internal(child)
    }

    native fun borrow_child_internal<T: key>(id: ID): &T;
    native fun borrow_child_mut_internal<T: key>(id: ID): &mut T;
    native fun take_child_internal<T: key>(id: ID): T;

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

}
