module 0x42::m {

    public enum Option<T> has drop {
        None,
        Some(T)
    }

    fun foo(): bool {
        false
    }

    fun t0(o: &mut Option<u64>): u64 {
        match (o) {
            _ if ({return 0; true}) => 1,
            Option::None => 2,
            _ => 10,
        }
    }
}
