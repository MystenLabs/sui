module a::m {
    use sui::tx_context;

    public entry fun foo<T>(_: T, _: &mut tx_context::TxContext) {
        abort 0
    }

}

module sui::tx_context {
    struct TxContext has drop {}
}
