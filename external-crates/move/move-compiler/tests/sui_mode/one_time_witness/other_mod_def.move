// invalid, one-time witness type candidate used in a different module

module a::n {
    use sui::sui;
    use sui::tx_context;

    fun init(_otw: sui::SUI, _ctx: &mut tx_context::TxContext) {
    }

}


module sui::tx_context {
    struct TxContext has drop {}
}

module sui::sui {
    struct SUI has drop {}
}
