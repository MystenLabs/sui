module a::m {
    public struct X(u64, u64) has copy, drop;

    fun f(t: X): u64 {
        t.0 + t.1
    }

    fun user(t: X) {
        use a::m::f as X;
        let _ = X(t);
    }
}
