// Two sequential `let ... else` bindings can introduce the same name; the
// second shadows the first. Correctness check for the binder-declaration
// path through typing + CFGIR.
module 0x42::m {

    public enum O<T> has drop {
        S(T),
        N,
    }

    fun shadowing(): u64 {
        let O::S(x) = O::S(1u64) else { return 0 };
        let O::S(x) = O::S(x + 10) else { return 0 };
        x
    }

}
