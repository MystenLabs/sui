/// FastX object identifiers
module FastX::ID {
    friend FastX::TxContext;

    /// Globally unique identifier of an object. This is a privileged type
    /// that can only be derived from a `TxContext`
    struct ID has store, drop {
        id: IDBytes
    }

    /// Underlying representation of an ID.
    /// Unlike ID, not a privileged type--can be freely copied and created
    struct IDBytes has store, drop, copy {
        bytes: vector<u8>
    }

    /// Create a new ID. Only callable by Lib
    public(friend) fun new(bytes: vector<u8>): ID {
        ID { id: IDBytes { bytes } }
    }

    /// Create a new ID bytes for comparison with existing ID's
    public fun new_bytes(bytes: vector<u8>): IDBytes {
        IDBytes { bytes }
    }

    /// Get the underyling `IDBytes` of `id`
    public fun get_inner(id: &ID): &IDBytes {
        &id.id
    }

    /// Get the `IDBytes` of `obj`
    public fun get_id_bytes<T: key>(obj: &T): &IDBytes {
        &get_id(obj).id
    }

    /// Get the raw bytes of `i`
    public fun get_bytes(i: &IDBytes): &vector<u8> {
        &i.bytes
    }

    /// Get the ID for `obj`. Safe because fastX has an extra
    /// bytecode verifier pass that forces every struct with
    /// the `key` ability to have a distinguished `ID` field.
    public native fun get_id<T: key>(obj: &T): &ID;
}
