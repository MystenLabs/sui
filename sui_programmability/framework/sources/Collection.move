module Sui::Collection {
    use Std::Errors;
    use Std::Option::{Self, Option};
    use Std::Vector::Self;
    use Sui::ID::{Self, ID, VersionedID};
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};

    // Error codes
    const EOBJECT_NOT_FOUND: u64 = 0;
    const EOBJECT_DOUBLE_ADD: u64 = 1;
    const EINVALID_MAX_CAPACITY: u64 = 2;
    const EMAX_CAPACITY_EXCEEDED: u64 = 3;

    // TODO: this is a placeholder number
    const DEFAULT_MAX_CAPACITY: u64 = 65536;

    // TODO: We should create a sepratate type called "Bag" to hold heterogeneous objects.
    // And keep Collection to take objects of the same type.
    struct Collection has key {
        id: VersionedID,
        objects: vector<ID>,
        max_capacity: u64,
    }

    /// Create a new Collection and return it.
    public fun new(ctx: &mut TxContext): Collection {
        new_with_max_capacity(ctx, DEFAULT_MAX_CAPACITY)
    }

    /// Create a new Collection with custom size limit and return it.
    public fun new_with_max_capacity(ctx: &mut TxContext, max_capacity: u64): Collection {
        assert!(
            max_capacity <= DEFAULT_MAX_CAPACITY && max_capacity > 0 ,
            Errors::limit_exceeded(EINVALID_MAX_CAPACITY)
        );
        Collection {
            id: TxContext::new_id(ctx),
            objects: Vector::empty(),
            max_capacity,
        }
    }

    /// Create a new Collection and transfer it to the signer.
    public fun create(ctx: &mut TxContext) {
        Transfer::transfer(new(ctx), TxContext::sender(ctx))
    }

    /// Returns the size of the collection.
    public fun size(c: &Collection): u64 {
        Vector::length(&c.objects)
    }

    /// Add a new object to the collection.
    /// Abort if the object is already in the collection.
    public fun add<T: key>(c: &mut Collection, object: T) {
        assert!(
            size(c) + 1 <= c.max_capacity,
            Errors::limit_exceeded(EMAX_CAPACITY_EXCEEDED)
        );
        let id = ID::id(&object);
        if (contains(c, id)) {
            abort EOBJECT_DOUBLE_ADD
        };
        Vector::push_back(&mut c.objects, *id);
        Transfer::transfer_to_object_unsafe(object, c);
    }

    /// Check whether the collection contains a specific object,
    /// identified by the object id in bytes.
    public fun contains(c: &Collection, id: &ID): bool {
        Option::is_some(&find(c, id))
    }

    /// Remove and return the object from the collection.
    /// Abort if the object is not found.
    public fun remove<T: key>(c: &mut Collection, object: T): T {
        let idx = find(c, ID::id(&object));
        if (Option::is_none(&idx)) {
            abort EOBJECT_NOT_FOUND
        };
        Vector::remove(&mut c.objects, *Option::borrow(&idx));
        object
    }

    /// Remove the object from the collection, and then transfer it to the signer.
    public fun remove_and_take<T: key>(c: &mut Collection, object: T, ctx: &mut TxContext) {
        let object = remove(c, object);
        Transfer::transfer(object, TxContext::sender(ctx));
    }

    /// Transfer the entire collection to `recipient`.
    /// This function can be called as an entry function, as it has TxContext as input.
    public fun transfer_entry(c: Collection, recipient: address, _ctx: &mut TxContext) {
        transfer(c, recipient)
    }

    /// Transfer the entire collection to `recipient`.
    public fun transfer(c: Collection, recipient: address) {
        Transfer::transfer(c, recipient)
    }

    /// Look for the object identified by `id_bytes` in the collection.
    /// Returns the index if found, none if not found.
    fun find(c: &Collection, id: &ID): Option<u64> {
        let i = 0;
        let len = size(c);
        while (i < len) {
            if (Vector::borrow(&c.objects, i) == id) {
                return Option::some(i)
            };
            i = i + 1;
        };
        return Option::none()
    }
}