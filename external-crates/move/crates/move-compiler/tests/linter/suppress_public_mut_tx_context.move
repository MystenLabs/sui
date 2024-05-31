module 0x42::suppress_cases {
    use sui::tx_context::TxContext;

    // No suppression, should trigger a warning
    public fun unsuppressed_function(_ctx: &TxContext) {
        // This should trigger a warning
    }

    // Suppress for a specific function
    #[allow(lint(require_mutable_tx_context))]
    public fun suppressed_function(_ctx: &TxContext) {
        // This should not trigger a warning
    }

    // Suppress multiple lints, including our target lint
    #[allow(lint(require_mutable_tx_context))]
    public fun multi_suppressed_function(_ctx: &TxContext) {
        // This should not trigger a warning
    }

    // Test suppression with multiple parameters
    #[allow(lint(require_mutable_tx_context))]
    public fun suppressed_multi_param(_a: u64, _ctx: &TxContext, _b: &mut TxContext) {
        // This should not trigger a warning, even though it has both mutable and immutable TxContext
    }
}

// Mocking the sui::tx_context module
module sui::tx_context {
    struct TxContext has drop {}
}
