module 0x42::m {

    public enum Pair<T> has drop {
        Both(T,T)
    }

    fun default<T: drop>(_o: Pair<T>): u64 {
        0
    }

    fun drop_ref<T>(_o: &Pair<T>) {
    }

    fun t0(): u64 {
        use 0x42::m::Pair as P;
        let o: P<P<u64>> = P::Both(P::Both(0, 1), P::Both(2, 3));
        let _y = &10;
        match (o) {
            P::Both(P::Both(m, n), P::Both(q, r)) if (&(*q + *r) == &5) => m + n + q + r,
            z => default(z),
        }
    }

}
