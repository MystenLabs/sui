// type parameters can have key
module a::m {
    use sui::tx_context;

    public entry fun t1<T: key>(_: T, _: &mut tx_context::TxContext) {
        abort 0
    }
    public entry fun t2<T: key>(_: &T, _: &mut tx_context::TxContext) {
        abort 0
    }
    public entry fun t3<T: key>(_: &mut T, _: &mut tx_context::TxContext) {
        abort 0
    }

}
module sui::tx_context {
    struct TxContext has drop {}
}
