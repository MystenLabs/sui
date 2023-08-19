// init is unused but does not error because we are in Sui mode
module a::m {
    fun init(_: &mut sui::tx_context::TxContext) {}
}

module sui::tx_context {
    struct TxContext has drop {}
}
