module 0x42::m {

    public enum Option<T> has drop {
        None,
        Some(T)
    }

    fun t0(): u64 {
        let o: Option<u64> = Option::None;
        match (o) {
            Option::Some(n) if (n == &5) => n,
            Option::None => 3,
            Option::Some(n) if (n == &3) => n,
            Option::Some(m) if (m == &2) => m,
            Option::Some(y) => y,
        }
    }
}
