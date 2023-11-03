//# init --edition 2024.alpha

//# publish
module 0x42::m {

    public enum Option<T> {
        Some(T),
        None
    }

    fun t0(): u64 {
        match (Option::Some(0)) {
            Option::Some(x) => x,
            Option::None => 1,
        }
    }

    fun t1(opt: Option<Option<u64>>): u64 {
        match (opt) {
            Option::Some(Option::Some(x)) => x,
            Option::Some(Option::None) => 1,
            Option::None => 2,
        }
    }

    fun t2(opt: &Option<Option<u64>>, default: &u64): &u64 {
        match (opt) {
            Option::Some(Option::Some(x)) => x,
            Option::Some(Option::None) => default,
            Option::None => default,
        }
    }

    fun t3(opt: &mut Option<Option<u64>>, default: &mut u64): &mut u64 {
        match (opt) {
            Option::Some(Option::Some(x)) => x,
            Option::Some(Option::None) => default,
            Option::None => default,
        }
    }

}
