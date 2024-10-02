module 0x42::m {

    public enum Option<T> has drop {
        Some { value: T },
        None
    }

    fun t0(): u64 {
        match (Option::Some { value: 0 }) {
            Option::Some { value } => value,
        }
    }

    fun t1(opt: Option<Option<u64>>): u64 {
        match (opt) {
            Option::Some { value: Option::None } => 1,
            Option::None => 2,
        }
    }

    fun t2(opt: Option<Option<u64>>): u64 {
        match (opt) {
            Option::Some { value: Option::Some { value: x } } => x,
        }
    }

    public enum Pair<T> has drop {
        P { one: T , two: T },
    }

    fun t3(pair: Pair<u64>): u64 {
        match (pair) {
            Pair::P { one, two: 0 } => one,
        }
    }

    fun t4(pair: Pair<u64>): u64 {
        match (pair) {
            Pair::P { two: 0, one } => one,
        }
    }

}
