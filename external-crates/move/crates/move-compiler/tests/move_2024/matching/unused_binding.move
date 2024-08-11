module 0x42::m {

    public enum Option<T> {
        Some(T),
        None
    }

    fun t0(): u64 {
        match (Option::Some(0)) {
            Option::Some(x) => 0, // x unused
            Option::None => 1,
        }
    }

    fun or_default<T: drop>(opt: Option<T>, default: T): T {
        match (opt) {
            Option::Some(x) => default, // x unused
            Option::None => default,
        }
    }
}
