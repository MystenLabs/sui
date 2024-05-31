module 0x42::false_negative {
    use sui::tx_context::TxContext;

    public fun tricky_function(condition: bool, ctx1: &TxContext, ctx2: &mut TxContext) {
        // This should ideally trigger a warning for ctx1, but might be missed
        if (condition) {
            use_context(ctx1);
        } else {
            use_context_mut(ctx2);
        }
    }

    fun use_context(_ctx: &TxContext) {}
    fun use_context_mut(_ctx: &mut TxContext) {}
}

module sui::tx_context {
    struct TxContext has drop {}
}
