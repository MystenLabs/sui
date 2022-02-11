module FastX::Collection {
    use Std::Option::{Self, Option};
    use Std::Vector::Self;
    use FastX::Address::{Self, Address};
    use FastX::ID::{Self, ID, IDBytes};
    use FastX::Transfer;
    use FastX::TxContext::{Self, TxContext};

    const OBJECT_NOT_FOUND: u64 = 0;
    const OBJECT_DOUBLE_ADD: u64 = 1;

    struct Collection has key {
        id: ID,
        objects: vector<IDBytes>,
    }

    /// Create a new Collection and return it.
    public fun new(ctx: &mut TxContext): Collection {
        Collection {
            id: TxContext::new_id(ctx),
            objects: Vector::empty<IDBytes>(),
        }
    }

    /// Create a new Collection and transfer it to the signer.
    public fun create(ctx: &mut TxContext) {
        Transfer::transfer(new(ctx), TxContext::get_signer_address(ctx))
    }

    /// Returns the size of the collection.
    public fun size(c: &Collection): u64 {
        Vector::length(&c.objects)
    }

    /// Add a new object to the collection.
    /// Abort if the object is already in the collection.
    public fun add<T: key>(c: &mut Collection, object: T) {
        let id_bytes = ID::get_id_bytes(&object);
        if (contains(c, id_bytes)) {
            abort OBJECT_DOUBLE_ADD
        };
        Vector::push_back(&mut c.objects, *id_bytes);
        Transfer::transfer_to_object(object, c);
    }

    /// Check whether the collection contains a specific object,
    /// identified by the object id in bytes.
    public fun contains(c: &Collection, id_bytes: &IDBytes): bool {
        Option::is_some(&find(c, id_bytes))
    }

    /// Remove and return the object from the collection.
    /// Abort if the object is not found.
    public fun remove<T: key>(c: &mut Collection, object: T): T {
        let idx = find(c, ID::get_id_bytes(&object));
        if (Option::is_none(&idx)) {
            abort OBJECT_NOT_FOUND
        };
        Vector::remove(&mut c.objects, *Option::borrow(&idx));
        object
    }

    /// Remove the object from the collection, and then transfer it to the signer.
    public fun remove_and_take<T: key>(c: &mut Collection, object: T, ctx: &mut TxContext) {
        let object = remove(c, object);
        Transfer::transfer(object, TxContext::get_signer_address(ctx));
    }

    /// Transfer the entire collection to `recipient`.
    /// This function can be called as an entry function, as it has TxContext as input.
    public fun transfer_entry(c: Collection, recipient: vector<u8>, _ctx: &mut TxContext) {
        transfer(c, Address::new(recipient))
    }

    /// Transfer the entire collection to `recipient`.
    public fun transfer(c: Collection, recipient: Address) {
        Transfer::transfer(c, recipient)
    }

    /// Look for the object identified by `id_bytes` in the collection.
    /// Returns the index if found, none if not found.
    fun find(c: &Collection, id_bytes: &IDBytes): Option<u64> {
        let i = 0;
        let len = size(c);
        while (i < len) {
            if (Vector::borrow(&c.objects, i) == id_bytes) {
                return Option::some(i)
            };
            i = i + 1;
        };
        return Option::none()
    }
}