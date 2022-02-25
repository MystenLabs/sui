module Sui::Transfer {
    use Sui::ID;

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

    /// Transfer ownership of `obj` to `recipient` and then freeze
    /// `obj`. After freezing `obj` becomes immutable and can no
    /// longer be transfered or mutated.
    /// If you just want to freeze an object, you can set the `recipient`
    /// to the current owner of `obj` and it will only be frozen without
    /// being transfered.
    public fun transfer_and_freeze<T: key>(obj: T, recipient: address) {
        transfer_internal(obj, recipient, true)
    }

    native fun transfer_internal<T: key>(obj: T, recipient: address, should_freeze: bool);

    /// Transfer ownership of `obj` to another object `owner`.
    // TODO: Add option to freeze after transfer.
    public fun transfer_to_object<T: key, R: key>(obj: T, owner: &mut R) {
        transfer_to_object_id(obj, *ID::get_bytes(ID::get_id_bytes(owner)));
    }

    /// Transfer ownership of `obj` to another object with `id`.
    native fun transfer_to_object_id<T: key>(obj: T, id: address);
}
