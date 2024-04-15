module 0x42::m {

    public enum Option<T> has drop {
        Some(T),
        None
    }

    fun t0(): u64 {
        match (Option::Some(0)) {
            Option::Some(x) if (x == 10) => x,
            Option::None => 2
        }
    }

}
