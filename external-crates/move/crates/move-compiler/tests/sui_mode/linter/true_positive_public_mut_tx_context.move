// tests the lint for preferring &mut TxContext over &TxContext in public functions
// these cases correctly should trigger the lint
module 0x42::true_positive {
    use sui::tx_context::TxContext;

    struct CustomStruct has drop {}

    public fun incorrect_mint(_ctx: &TxContext) {
    }

    public fun another_incorrect(_a: u64, _b: &TxContext, _c: u64) {
    }

    public fun mixed_function(_a: &CustomStruct, _b: &TxContext, _c: &mut TxContext) {}

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
