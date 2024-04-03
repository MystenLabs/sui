module 0x42::m {

    public enum Option<T> has drop {
        Some(T),
        None
    }

    fun t0(): u64 {
        match (Option::Some(0)) {
            Option::Some(x) => x,
        }
    }

    fun t1(opt: Option<Option<u64>>): u64 {
        match (opt) {
            Option::Some(Option::None) => 1,
            Option::None => 2,
        }
    }

    fun t2(opt: Option<Option<u64>>): u64 {
        match (opt) {
            Option::Some(Option::Some(x)) => x,
        }
    }

}
