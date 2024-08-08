module 0x42::false_positive {
    struct CustomContext has drop {}

    public fun custom_context_function(_ctx: &CustomContext) {
        // This should not trigger a warning, as it's not TxContext
    }

    public fun generic_function<T: drop>(_ctx: &T) {
        // This should not trigger a warning, as it's a generic type
    }
}

module sui::tx_context {
    struct TxContext has drop {}
}
