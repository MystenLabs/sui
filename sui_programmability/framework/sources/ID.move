/// FastX object identifiers
module FastX::ID {
    use Std::BCS;
    // TODO(): bring this back
    //friend FastX::TxContext;

    /// Version of a ID created by the current transaction.
    const INITIAL_VERSION: u64 = 0;

    /// Globally unique identifier of an object. This is a privileged type
    /// that can only be derived from a `TxContext`
    /// ID doesn't have drop capability, which means to delete an ID (when
    /// deleting an object), one must explicitly call the delete function.
    struct ID has store {
        id: IDBytes,
        /// Version number for the ID. The version number is incremented each
        /// time the object with this ID is passed to a non-failing transaction
        /// either by value or by mutable reference.
        /// Note: if the object with this ID gets wrapped in another object, the
        /// child object may be mutated with no version number change.
        version: u64
    }

    /// Underlying representation of an ID.
    /// Unlike ID, not a privileged type--can be freely copied and created
    struct IDBytes has store, drop, copy {
        bytes: address
    }

    /// Create a new ID. Only callable by TxContext
    // TODO (): bring this back once we can support `friend`
    //public(friend) fun new(bytes: vector<u8>): ID {
    public fun new(bytes: address): ID {
        ID { id: IDBytes { bytes }, version: INITIAL_VERSION }
    }

    /// Create a new ID bytes for comparison with existing ID's.
    public fun new_bytes(bytes: vector<u8>): IDBytes {
        IDBytes { bytes: bytes_to_address(bytes) }
    }

    /// Get the underyling `IDBytes` of `id`
    public fun get_inner(id: &ID): &IDBytes {
        &id.id
    }

    /// Get the `IDBytes` of `obj`
    public fun get_id_bytes<T: key>(obj: &T): &IDBytes {
        &get_id(obj).id
    }

    /// Get the `version` of `obj`
    public fun get_version<T: key>(obj: &T): u64 {
        *&get_id(obj).version
    }

    /// Return `true` if `obj` was created by the current transaction,
    /// `false` otherwise.
    public fun created_by_current_tx<T: key>(obj: &T): bool {
        get_version(obj) == INITIAL_VERSION
    }

    /// Get the raw bytes of `i` in its underlying representation
    // TODO: we should probably not expose that this is an `address`
    public fun get_bytes(i: &IDBytes): &address {
        &i.bytes
    }

    /// Get the raw bytes of `i` as a vector
    public fun get_bytes_as_vec(i: &IDBytes): vector<u8> {
        BCS::to_bytes(get_bytes(i))
    }

    /// Get the ID for `obj`. Safe because fastX has an extra
    /// bytecode verifier pass that forces every struct with
    /// the `key` ability to have a distinguished `ID` field.
    public native fun get_id<T: key>(obj: &T): &ID;

    public native fun bytes_to_address(bytes: vector<u8>): address;

    /// When an object is being deleted through unpacking, the 
    /// delete function must be called on the id to inform Sui
    /// regarding the deletion of the object.
    public native fun delete(id: ID);
}
