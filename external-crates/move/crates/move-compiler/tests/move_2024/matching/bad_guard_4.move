module 0x42::m {

    public enum Option<T> has drop {
        None,
        Some(T)
    }

    fun foo(): bool {
        false
    }

    fun t0(): u64 {
        let o = &mut Option::Some(0u64);
        match (o) {
            Option::Some(_) => 0,
            x if ({*x = Option::Some(1u64); false}) => 1,
            Option::None => 2,
            x => 10,
        }
    }
}
