module 0x42::true_positive {
    use sui::tx_context::TxContext;

    public fun incorrect_mint(_ctx: &TxContext) {
        // This should trigger a warning
    }

    public fun another_incorrect(_a: u64, _b: &TxContext, _c: u64) {
        // This should also trigger a warning
    }
}

module sui::tx_context {
    struct TxContext has drop {}
}
