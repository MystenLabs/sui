module 0x42::m {

    public enum Option<T> {
        Some(T),
        None
    }

    fun or_default<T: drop>(opt: Option<T>, default: T): T {
        match (opt) {
            Option::Some(x) => x,
            Option::None => default,
        }
    }
}
