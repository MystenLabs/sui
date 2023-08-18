// structs are always given a field in the source language

module a::m {
    use sui::tx_context;

    struct M has drop {}

    fun init(_otw: M, _ctx: &mut tx_context::TxContext) {
    }

}

module sui::tx_context {
    struct TxContext has drop {}
}
