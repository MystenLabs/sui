// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A Bag is a heterogeneous collection of objects with arbitrary types, i.e.
/// the objects in the bag don't need to be of the same type.
/// These objects are not stored in the Bag directly, instead only a reference
/// to their IDs are stored as a proof of ownership. Sui tracks the ownership
/// and is aware that the Bag owns those objects in it. Only the owner of the Bag
/// could mutate the objects in the Bag.
/// Bag is different from the Collection type in that Collection
/// only supports owning objects of the same type.
module sui::bag {
    use sui::id::{Self, ID, VersionedID};
    use sui::transfer;
    use sui::typed_id::{Self, TypedID};
    use sui::tx_context::{Self, TxContext};
    use sui::vec_set::{Self, VecSet};

    // Error codes
    /// Adding the same object to the collection twice is not allowed.
    const EObjectDoubleAdd: u64 = 0;

    /// The max capacity set for the collection cannot exceed the hard limit
    /// which is DEFAULT_MAX_CAPACITY.
    const EInvalidMaxCapacity: u64 = 1;

    /// Trying to add object to the collection when the collection is
    /// already at its maximum capacity.
    const EMaxCapacityExceeded: u64 = 2;

    // TODO: this is a placeholder number
    const DEFAULT_MAX_CAPACITY: u64 = 65536;

    struct Bag has key {
        id: VersionedID,
        objects: VecSet<ID>,
        max_capacity: u64,
    }

    struct Item<T: store> has key {
        id: VersionedID,
        value: T,
    }

    /// Create a new Bag and return it.
    public fun new(ctx: &mut TxContext): Bag {
        new_with_max_capacity(ctx, DEFAULT_MAX_CAPACITY)
    }

    /// Create a new Bag with custom size limit and return it.
    public fun new_with_max_capacity(ctx: &mut TxContext, max_capacity: u64): Bag {
        assert!(max_capacity <= DEFAULT_MAX_CAPACITY && max_capacity > 0, EInvalidMaxCapacity);
        Bag {
            id: tx_context::new_id(ctx),
            objects: vec_set::empty(),
            max_capacity,
        }
    }

    /// Create a new Bag and transfer it to the signer.
    public entry fun create(ctx: &mut TxContext) {
        transfer::transfer(new(ctx), tx_context::sender(ctx))
    }

    /// Returns the size of the Bag.
    public fun size(c: &Bag): u64 {
        vec_set::size(&c.objects)
    }

    /// Add a new object to the Bag.
    public fun add<T: store>(c: &mut Bag, value: T, ctx: &mut TxContext): TypedID<Item<T>> {
        assert!(size(c) + 1 <= c.max_capacity, EMaxCapacityExceeded);
        let id = tx_context::new_id(ctx);
        vec_set::insert(&mut c.objects, *id::inner(&id));
        let item = Item { id, value };
        let item_id = typed_id::new(&item);
        transfer::transfer_to_object(item, c);
        item_id
    }

    /// identified by the object id in bytes.
    public fun contains(c: &Bag, id: &ID): bool {
        vec_set::contains(&c.objects, id)
    }

    /// Remove and return the object from the Bag.
    /// Abort if the object is not found.
    public fun remove<T: store>(c: &mut Bag, item: Item<T>): T {
        let Item { id, value } = item;
        vec_set::remove(&mut c.objects, id::inner(&id));
        id::delete(id);
        value
    }

    /// Remove the object from the Bag, and then transfer it to the signer.
    public entry fun remove_and_take<T: key + store>(
        c: &mut Bag,
        item: Item<T>,
        ctx: &mut TxContext,
    ) {
        let object = remove(c, item);
        transfer::transfer(object, tx_context::sender(ctx));
    }

    /// Transfer the entire Bag to `recipient`.
    public entry fun transfer(c: Bag, recipient: address) {
        transfer::transfer(c, recipient)
    }

    public fun transfer_to_object_id(
        obj: Bag,
        owner_id: &VersionedID,
    ) {
        transfer::transfer_to_object_id(obj, owner_id)
    }
}
