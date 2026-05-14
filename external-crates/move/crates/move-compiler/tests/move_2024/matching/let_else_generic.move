module 0x42::m {

    public enum Option<T> has drop {
        Some(T),
        None,
    }

    fun unwrap_or_default<T: drop>(opt: Option<T>, default: T): T {
        let Option::Some(val) = opt else { return default };
        val
    }

}
