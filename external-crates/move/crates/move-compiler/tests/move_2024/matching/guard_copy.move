module 0x42::m {

    public enum Option<T> has drop {
        None,
        Some(T)
    }

    public struct S has copy, drop {}

    fun check_s(_s: S): bool {
        false
    }

    fun t0(): S {
        let o: Option<S> = Option::None;
        match (o) {
            Option::Some(n) if check_s(copy n) => n,
            Option::Some(y) => y,
            Option::None => S {},
        }
    }
}
