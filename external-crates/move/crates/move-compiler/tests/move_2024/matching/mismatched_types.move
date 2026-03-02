module 0x42::m {
    public struct P(bool) has drop;
    public struct N { x: bool } has drop;

    fun f(n: N): bool {
        match (n) {
            P(false) => false,
        }
    }
}
