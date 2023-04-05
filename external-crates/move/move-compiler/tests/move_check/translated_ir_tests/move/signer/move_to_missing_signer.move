module 0x8675309::M {
    struct R has key { f: bool }
    fun t0(_: &signer) {
        move_to<R>(R { f: false })
    }
}
// check: NEGATIVE_STACK_SIZE_WITHIN_BLOCK

//! new-transaction

module 0x8675309::N {
    struct R<T> has key { f: T }
    fun t0<T>(_: &signer) {
        () = move_to<R<bool>>(R<bool> { f: false });
    }
}
// check: NEGATIVE_STACK_SIZE_WITHIN_BLOCK
