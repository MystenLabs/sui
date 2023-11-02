module a::beep {
    struct BEEP has drop {
        f0: u64,
        f1: bool,
    }
    fun init(_: BEEP, _ctx: &mut sui::tx_context::TxContext) {
    }
}

module sui::tx_context {
    struct TxContext has drop {}
}
