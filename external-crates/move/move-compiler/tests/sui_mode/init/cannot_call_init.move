// cannot directly call init
module a::m {
    use sui::tx_context;
    fun init(_ctx: &mut tx_context::TxContext) {
        abort 0
    }

    public entry fun init_again(ctx: &mut tx_context::TxContext) {
        init(ctx)
    }
}

module sui::tx_context {
    struct TxContext has drop {}
}
