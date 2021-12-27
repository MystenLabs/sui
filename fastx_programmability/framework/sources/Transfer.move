module FastX::Transfer {
    use FastX::Address::{Self, Address};
    //use FastX::ID::IDBytes;

    /// Transfers are implemented by emitting a
    /// special `TransferEvent` that the fastX adapter
    /// interprets differently than user events.
    struct TransferEvent<T: key> {
        /// The object to be transferred
        obj: T,
        /// Address the object will be
        /// transferred to.
        recipient: Address,
    }

    /// Transfer ownership of `obj` to `recipient`. `obj` must have the
    /// `key` attribute, which (in turn) ensures that `obj` has a globally
    /// unique ID.
    // TODO: add bytecode verifier pass to ensure that `T` is a struct declared
    // in the calling module. This will allow modules to define custom transfer
    // logic for their structs that cannot be subverted by other modules
    public fun transfer<T: key>(obj: T, recipient: Address) {
        // TODO: emit event
        transfer_internal(obj, Address::into_bytes(recipient))
    }

    native fun transfer_internal<T: key>(obj: T, recipient: vector<u8>);

    /*/// Transfer ownership of `obj` to another object `id`. Afterward, `obj`
    /// can only be used in a transaction that also includes the object with
    /// `id`.
    /// WARNING: Use with caution. Improper use can create ownership cycles
    /// between objects, which will cause all objects involved in the cycle to
    /// be locked.
    public native fun transfer_to_id<T: key>(obj: T, id: IDBytes);*/
}
