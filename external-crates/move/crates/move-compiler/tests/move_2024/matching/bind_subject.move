module 0x42::m {

    public enum Option<T> has drop {
        None,
        Some(T)
    }

    fun default<T: drop>(_o: Option<T>): u64 {
        0
    }

    fun t0(): u64 {
        let o: Option<u64> = Option::None;
        match (o) {
            Option::None => 3,
            z => default(z),
        }
    }
}
