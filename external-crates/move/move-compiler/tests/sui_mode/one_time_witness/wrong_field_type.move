// correct, wrong struct field type but not one-time witness candidate

module a::m {
    use sui::tx_context;

    struct M has drop { value: u64 }

    fun init(_ctx: &mut tx_context::TxContext) {
    }

    fun foo() {
        _ = M { value: 7 };
        _ = M { value: 42 };
    }
}

module sui::tx_context {
    struct TxContext has drop {}
}
