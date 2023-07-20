// correct, bool field specified at source level

module a::m {
    use sui::tx_context;

    struct M has drop { some_field: bool }

    fun init(_otw: M, _ctx: &mut tx_context::TxContext) {
        return;
    }
}

module sui::tx_context {
    struct TxContext has drop {}
}
