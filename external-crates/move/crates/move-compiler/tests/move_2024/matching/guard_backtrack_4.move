module 0x42::m {

    public enum Option<T> has drop {
        None,
        Some(T)
    }

    fun default<T: drop>(_o: Option<T>): u64 {
        0
    }

    fun t0(): u64 {
        let o: Option<Option<u64>> = Option::None;
        let _y = &10;
        match (o) {
            Option::Some(Option::Some(n)) if (_y == &5) => n,
            Option::Some(q) if (_y == &5) => default(q),
            Option::None => 1,
            z => default(z),
        }
    }


}
