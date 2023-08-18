// invalid, wrong struct field type

module a::m {
    use sui::tx_context;

    struct M has drop { value: u64 }

    fun init(_otw: M, _ctx: &mut tx_context::TxContext) {
    }
}

module sui::tx_context {
    struct TxContext has drop {}
}
