module 0x42::true_negative {
    use sui::tx_context::TxContext;

    public fun correct_mint(_ctx: &mut TxContext) {
        // This should not trigger a warning
    }

    public fun another_correct(_a: u64, _b: &mut TxContext, _c: u64) {
        // This should also not trigger a warning
    }

    fun private_function(_ctx: &TxContext) {
        // This should not trigger a warning as it's private
    }
}

module sui::tx_context {
    struct TxContext has drop {}
}
