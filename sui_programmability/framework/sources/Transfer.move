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

    /// Transfer ownership of `obj` to another object `owner`.
    public fun transfer_to_object<T: key, R: key>(obj: T, owner: &mut R) {
        let owner_id = ID::id_address(owner);
        transfer_internal(obj, owner_id, true);
    }

    /// Freeze `obj`. After freezing `obj` becomes immutable and can no
    /// longer be transfered or mutated.
    public native fun freeze_object<T: key>(obj: T);

    native fun transfer_internal<T: key>(obj: T, recipient: address, to_object: bool);
}
