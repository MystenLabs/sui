// init functions cannot be public, cannot have entry, cannot be public(friend)
module a::m0 {
    use sui::tx_context;
    public fun init(_ctxctx: &mut tx_context::TxContext) {
        abort 0
    }
}

module a::m1 {
    use sui::tx_context;
    entry fun init(_ctx: &mut tx_context::TxContext) {
        abort 0
    }
}

module a::m2 {
    use sui::tx_context;
    public(friend) fun init(_ctx: &mut tx_context::TxContext) {
        abort 0
    }
}

module sui::tx_context {
    struct TxContext has drop {}
}
