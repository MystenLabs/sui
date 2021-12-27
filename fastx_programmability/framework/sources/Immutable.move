module FastX::Immutable {
    use FastX::ID::ID;
    use FastX::Transfer;
    use FastX::TxContext::{Self, TxContext};

    /// An immutable, non-transferrable object.
    /// Unlike mutable objects, immutable objects can read by *any*
    /// transaction, not only one signed by the object's owner.
    /// The authenticator associated with an immutable object is
    /// always its creator
    struct Immutable<T: store> has key {
        id: ID,
        /// Abritrary immutable data associated with the object
        data: T
    }

    /// Create an immutable object wrapping `data` owned by `ctx.signer`
    public fun create<T: store>(data: T, ctx: &mut TxContext) {
        let id = TxContext::new_id(ctx);
        let obj = Immutable { id, data };
        Transfer::transfer(obj, TxContext::get_signer_address(ctx))
    }
}
