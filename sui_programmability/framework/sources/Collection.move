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
module Sui::Collection {
    use Std::Errors;
    use Std::Option::{Self, Option};
    use Std::Vector::Self;
    use Sui::ID::{Self, ID, VersionedID};
    use Sui::Transfer::{Self, ChildRef};
    use Sui::TxContext::{Self, TxContext};

    // Error codes
    /// When removing an object from the collection, EOBJECT_NOT_FOUND
    /// will be triggered if the object is not owned by the collection.
    const EOBJECT_NOT_FOUND: u64 = 0;

    /// Adding the same object to the collection twice is not allowed.
    const EOBJECT_DOUBLE_ADD: u64 = 1;

    /// The max capacity set for the collection cannot exceed the hard limit
    /// which is DEFAULT_MAX_CAPACITY.
    const EINVALID_MAX_CAPACITY: u64 = 2;

    /// Trying to add object to the collection when the collection is
    /// already at its maximum capacity.
    const EMAX_CAPACITY_EXCEEDED: u64 = 3;

    // TODO: this is a placeholder number
    // We want to limit the capacity of collection because it requires O(N)
    // for search and removals. We could relax the capacity constraint once
    // we could use more efficient data structure such as set.
    const DEFAULT_MAX_CAPACITY: u64 = 65536;

    struct Collection<phantom T: key> has key {
        id: VersionedID,
        objects: vector<ChildRef<T>>,
        max_capacity: u64,
    }

    /// Create a new Collection and return it.
    public fun new<T: key>(ctx: &mut TxContext): Collection<T> {
        new_with_max_capacity(ctx, DEFAULT_MAX_CAPACITY)
    }

    /// Create a new Collection with custom size limit and return it.
    public fun new_with_max_capacity<T: key>(ctx: &mut TxContext, max_capacity: u64): Collection<T> {
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
    public fun create<T: key>(ctx: &mut TxContext) {
        Transfer::transfer(new<T>(ctx), TxContext::sender(ctx))
    }

    /// Returns the size of the collection.
    public fun size<T: key>(c: &Collection<T>): u64 {
        Vector::length(&c.objects)
    }

    /// Add a new object to the collection.
    /// Abort if the object is already in the collection.
    public fun add<T: key>(c: &mut Collection<T>, object: T) {
        assert!(
            size(c) + 1 <= c.max_capacity,
            Errors::limit_exceeded(EMAX_CAPACITY_EXCEEDED)
        );
        let id = ID::id(&object);
        assert!(!contains(c, id), EOBJECT_DOUBLE_ADD);
        let child_ref = Transfer::transfer_to_object(object, c);
        Vector::push_back(&mut c.objects, child_ref);
    }

    /// Check whether the collection contains a specific object,
    /// identified by the object id in bytes.
    public fun contains<T: key>(c: &Collection<T>, id: &ID): bool {
        Option::is_some(&find(c, id))
    }

    /// Remove and return the object from the collection.
    /// Abort if the object is not found.
    public fun remove<T: key>(c: &mut Collection<T>, object: T): (T, ChildRef<T>) {
        let idx = find(c, ID::id(&object));
        assert!(Option::is_some(&idx), EOBJECT_NOT_FOUND);
        let child_ref = Vector::remove(&mut c.objects, *Option::borrow(&idx));
        (object, child_ref)
    }

    /// Remove the object from the collection, and then transfer it to the signer.
    public fun remove_and_take<T: key>(c: &mut Collection<T>, object: T, ctx: &mut TxContext) {
        let (object, child_ref) = remove(c, object);
        Transfer::transfer_child_to_address(object, child_ref, TxContext::sender(ctx));
    }

    /// Transfer the entire collection to `recipient`.
    public fun transfer<T: key>(c: Collection<T>, recipient: address, _ctx: &mut TxContext) {
        Transfer::transfer(c, recipient)
    }

    /// Look for the object identified by `id_bytes` in the collection.
    /// Returns the index if found, none if not found.
    fun find<T: key>(c: &Collection<T>, id: &ID): Option<u64> {
        let i = 0;
        let len = size(c);
        while (i < len) {
            let child_ref = Vector::borrow(&c.objects, i);
            if (Transfer::child_id(child_ref) == id) {
                return Option::some(i)
            };
            i = i + 1;
        };
        return Option::none()
    }
}
