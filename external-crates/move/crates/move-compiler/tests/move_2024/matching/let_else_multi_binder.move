// Multi-binder positional variant: all binders in one pattern must be bound
// and visible to subsequent code. Existing tests only cover single-binder
// variants.
module 0x42::m {

    public enum Triple<T> has drop {
        T(T, T, T),
        N,
    }

    fun multi_binder(t: Triple<u64>): u64 {
        let Triple::T(a, b, c) = t else { return 0 };
        a + b + c
    }

}
