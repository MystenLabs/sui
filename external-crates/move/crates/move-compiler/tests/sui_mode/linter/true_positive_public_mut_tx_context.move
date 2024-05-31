module 0x42::true_positive {
    use sui::tx_context::TxContext;

    struct CustomStruct has drop {}

    public fun incorrect_mint(_ctx: &TxContext) {
        // This should trigger a warning
    }

    public fun another_incorrect(_a: u64, _b: &TxContext, _c: u64) {
        // This should also trigger a warning
    }

    public fun correct_function(_ctx: &mut TxContext) {}

    public fun incorrect_function(_ctx: &TxContext) {} // Should warn

    public fun mixed_function(_a: &CustomStruct, _b: &TxContext, _c: &mut TxContext) {} // Should warn for _b

    public fun generic_function<T: drop>(_ctx: &T) {}

    fun private_function(_ctx: &TxContext) {}

    public fun complex_function<T: drop>(
        _a: u64,
        _b: &TxContext, // Should warn
        _c: &mut TxContext,
        _d: &T,
        _e: &CustomStruct
    ) {}
}

module sui::tx_context {
    struct TxContext has drop {}
}
