module 0x42::m {

    public enum Option<T> has drop {
        None,
        Some(T)
    }

    fun foo(): bool {
        false
    }

    fun t0(): u64 {
        let o = &mut Option::Some(0);
        match (o) {
            Option::None => 0,
            _ if (foo()) => 1,
            Option::Some(_) => 2,
        }
    }
}
