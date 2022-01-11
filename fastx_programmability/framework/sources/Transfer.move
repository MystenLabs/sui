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
    public fun transfer<T: key>(obj: T, recipient: Address) {
        // TODO: emit event
        transfer_internal(obj, Address::into_bytes(recipient), false)
    }

    /// Transfer ownership of `obj` to `recipient` and then freeze
    /// `obj`. After freezing `obj` becomes immutable and can no
    /// longer be transfered or mutated.
    /// If you just want to freeze an object, you can set the `recipient`
    /// to the current owner of `obj` and it will only be frozen without
    /// being transfered.
    public fun transfer_and_freeze<T: key>(obj: T, recipient: Address) {
        transfer_internal(obj, Address::into_bytes(recipient), true)
    }

    native fun transfer_internal<T: key>(obj: T, recipient: vector<u8>, should_freeze: bool);

    /*/// Transfer ownership of `obj` to another object `id`. Afterward, `obj`
    /// can only be used in a transaction that also includes the object with
    /// `id`.
    /// WARNING: Use with caution. Improper use can create ownership cycles
    /// between objects, which will cause all objects involved in the cycle to
    /// be locked.
    public native fun transfer_to_id<T: key>(obj: T, id: IDBytes);*/
}
