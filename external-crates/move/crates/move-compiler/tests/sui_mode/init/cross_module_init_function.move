// check that fun `init` can be used cross module in sui-mode
module 0x1::M {
    fun init(_ctx: &mut sui::tx_context::TxContext) { }
}

module 0x1::Tests {
    #[test]
    fun tester() {
        use 0x1::M;
        let ctx = sui::tx_context::TxContext {};
        M::init(&ctx);
    }
}

module sui::tx_context {
    struct TxContext has drop {}
}
