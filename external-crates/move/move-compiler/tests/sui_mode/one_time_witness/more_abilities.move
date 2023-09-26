// invalid, struct type has abilities beyond drop

module a::m1 {
    use sui::tx_context;

    struct M1 has drop, copy { dummy: bool }

    fun init(_otw: M1, _ctx: &mut tx_context::TxContext) {
    }

}

module a::m2 {
    use sui::tx_context;

    struct M2 has drop, store { dummy: bool }

    fun init(_otw: M2, _ctx: &mut tx_context::TxContext) {
    }

}

module a::m3 {
    use sui::tx_context;

    struct M3 has drop, copy, store { dummy: bool }

    fun init(_otw: M3, _ctx: &mut tx_context::TxContext) {
    }

}

module sui::tx_context {
    struct TxContext has drop {}
}
