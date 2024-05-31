module 0x42::M {
    use sui::tx_context::TxContext;
    public fun mint(_ctx: &mut TxContext) {
        
    }
}

module sui::tx_context {
    struct TxContext has drop {}
}