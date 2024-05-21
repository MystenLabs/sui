module a::m {

    public struct P(u64) has drop;

    public struct N { x: u64 } has drop;

    fun t0(p: P, n: N): u64 {
        match (p) {
            P { x } => x,
        };
        match (n) {
            N(x) => x,
        }
    }
}
