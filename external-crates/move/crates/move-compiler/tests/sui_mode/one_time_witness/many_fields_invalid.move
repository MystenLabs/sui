// Incorrect, more than one field means not a OTW
module a::m {
    use sui::tx_context;

    struct M has drop { some_field: bool, some_field2: bool  }

    fun init(_otw: M, _ctx: &mut tx_context::TxContext) {
    }
}

module sui::tx_context {
    struct TxContext has drop {}
}
