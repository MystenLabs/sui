// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// The `Collection` type represents a collection of objects of the same type `T`.
/// In contrast to `vector<T>` which stores the object in the vector directly,
/// `Collection<T>` only tracks the ownership indirectly, by keeping a list of
/// references to the object IDs.
/// When using `vector<T>`, since the objects will be wrapped inside the vector,
/// these objects will not be stored in the global object pool, and hence not
/// directly accessible.
/// Collection allows us to own a list of same-typed objects, but still able to
/// access and operate on each individual object.
/// In contrast to `Bag`, `Collection` requires all objects have the same type.
module sui::collection {
    use sui::id::{Self, ID, VersionedID};
    use sui::transfer;
    use sui::typed_id::{Self, TypedID};
    use sui::tx_context::{Self, TxContext};
    use sui::vec_set::{Self, VecSet};

    // Error codes

    /// The max capacity set for the collection cannot exceed the hard limit
    /// which is DEFAULT_MAX_CAPACITY.
    const EInvalidMaxCapacity: u64 = 0;

    /// Trying to add object to the collection when the collection is
    /// already at its maximum capacity.
    const EMaxCapacityExceeded: u64 = 1;

    // TODO: this is a placeholder number
    // We want to limit the capacity of collection because it requires O(N)
    // for search and removals. We could relax the capacity constraint once
    // we could use more efficient data structure such as set.
    const DEFAULT_MAX_CAPACITY: u64 = 0x10000;

    struct Collection<phantom T: store> has key {
        id: VersionedID,
        objects: VecSet<ID>,
        max_capacity: u64,
    }

    struct Item<T: store> has key {
        id: VersionedID,
        value: T,
    }

    /// Create a new Collection and return it.
    public fun new<T: store>(ctx: &mut TxContext): Collection<T> {
        new_with_max_capacity(ctx, DEFAULT_MAX_CAPACITY)
    }

    /// Create a new Collection with custom size limit and return it.
    public fun new_with_max_capacity<T: store>(
        ctx: &mut TxContext,
        max_capacity: u64,
    ): Collection<T> {
        assert!(max_capacity <= DEFAULT_MAX_CAPACITY && max_capacity > 0, EInvalidMaxCapacity);
        Collection {
            id: tx_context::new_id(ctx),
            objects: vec_set::empty(),
            max_capacity,
        }
    }

    /// Create a new Collection and transfer it to the signer.
    public entry fun create<T: store>(ctx: &mut TxContext) {
        transfer::transfer(new<T>(ctx), tx_context::sender(ctx))
    }

    /// Returns the size of the collection.
    public fun size<T: store>(c: &Collection<T>): u64 {
        vec_set::size(&c.objects)
    }

    /// Add an object to the collection.
    public fun add<T: store>(
        c: &mut Collection<T>,
        value: T,
        ctx: &mut TxContext,
    ): TypedID<Item<T>> {
        assert!(size(c) + 1 <= c.max_capacity, EMaxCapacityExceeded);
        let id = tx_context::new_id(ctx);
        vec_set::insert(&mut c.objects, *id::inner(&id));
        let item = Item { id, value };
        let item_id = typed_id::new(&item);
        transfer::transfer_to_object(item, c);
        item_id
    }

    /// Check whether the collection contains a specific object,
    /// identified by the object id in bytes.
    public fun contains<T: store>(c: &Collection<T>, id: &ID): bool {
        vec_set::contains(&c.objects, id)
    }

    /// Remove and return the object from the collection.
    /// Abort if the object is not found.
    public fun remove<T: store>(c: &mut Collection<T>, item: Item<T>): T {
        let Item { id, value } = item;
        vec_set::remove(&mut c.objects, id::inner(&id));
        id::delete(id);
        value
    }

    /// Remove the object from the collection, and then transfer it to the signer.
    public entry fun remove_and_take<T: key + store>(
        c: &mut Collection<T>,
        item: Item<T>,
        ctx: &mut TxContext,
    ) {
        let object = remove(c, item);
        transfer::transfer(object, tx_context::sender(ctx));
    }

    /// Transfer the entire collection to `recipient`.
    public entry fun transfer<T: key + store>(c: Collection<T>, recipient: address) {
        transfer::transfer(c, recipient)
    }

    public fun transfer_to_object_id<T: key + store>(
        obj: Collection<T>,
        owner_id: &VersionedID,
    ) {
        transfer::transfer_to_object_id(obj, owner_id)
    }

}
