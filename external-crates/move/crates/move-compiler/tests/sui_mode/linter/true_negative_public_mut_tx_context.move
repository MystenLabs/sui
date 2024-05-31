module 0x42::true_negative {
    use sui::tx_context::TxContext;
    use sui::mock_tx_context::TxContext as SuiMockTxContext;

    public fun correct_mint(_ctx: &mut TxContext) {
        // This should not trigger a warning
    }

    public fun another_correct(_a: u64, _b: &mut TxContext, _c: u64) {
        // This should also not trigger a warning
    }

    fun private_function(_ctx: &TxContext) {
        // This should not trigger a warning as it's private
    }

    public fun custom_module(_b: &mut SuiMockTxContext) {}


}

module sui::tx_context {
    struct TxContext has drop {}
}

module sui::mock_tx_context {
    struct TxContext has drop {}
}
