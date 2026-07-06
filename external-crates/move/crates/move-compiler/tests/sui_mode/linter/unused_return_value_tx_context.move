
// TxContext is not considered a "mutating" input for the unused return value lint

module a::tests {
    use sui::tx_context::TxContext;

    fun mut_ctx(_ctx: &mut TxContext): u64 { 0 }

    fun t(ctx: &mut TxContext) {
        mut_ctx(ctx);
    }
}

module sui::tx_context {
    struct TxContext has drop {}
}
