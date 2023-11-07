module a::m {
    public struct X(u64) has copy, drop;
    public struct Y(bool, bool) has copy, drop;

    fun caller() {
        use a::m::X as Y;
        let x = X(0u64);
        let y = Y(1u64);
        f(x);
        f(y);
    }

    fun f(x: X) {
        use a::m::X as Y;
        let Y(_) = x;
        Y(_) = x;
        X(_) = x;
    }
}
