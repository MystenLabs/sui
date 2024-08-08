module 0x42::custom_tx_context {
    use sui::tx_context::TxContext as SuiTxContext;
    use sui::mock_tx_context::TxContext as SuiMockTxContext;

    // Custom TxContext struct, not from sui::tx_context
    struct TxContext has drop {}

    // This should trigger a warning (using Sui's TxContext)
    public fun sui_tx_function(_ctx: &SuiTxContext) {}

    // This should NOT trigger a warning (using custom TxContext)
    public fun custom_tx_function(_ctx: &TxContext) {}

    // This should trigger a warning (using Sui's TxContext)
    public fun mixed_function(_a: &TxContext, _b: &SuiTxContext) {}

    // This should NOT trigger a warning (both are custom TxContext)
    public fun double_custom(_a: &TxContext, _b: &mut TxContext) {}

    // This should NOT trigger a warning
    public fun custom_module(_a: &TxContext, _b: &mut SuiMockTxContext) {}
}

// Mocking the sui::tx_context module
module sui::tx_context {
    struct TxContext has drop {}
}

module sui::mock_tx_context {
    struct TxContext has drop {}
}
