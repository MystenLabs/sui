module 0x42::m {

    public enum Option<T> has drop {
        None,
        Some(T)
    }

    fun default<T: drop>(_o: Option<T>): u64 {
        0
    }

    // fun t0(): u64 {
    //     let o: Option<u64> = Option::None;
    //     match (o) {
    //         Option::Some(n) if (n == &5) => n,
    //         Option::None => 1,
    //         z => default(z),
    //     }
    // }

    // fun t1(): u64 {
    //     let o: Option<Option<u64>> = Option::None;
    //     match (o) {
    //         Option::Some(Option::Some(n)) if (n == &5) => n,
    //         Option::None => 3,
    //         z => default(z),
    //     }
    // }

    fun t2(): u64 {
        let o: Option<u64> = Option::None;
        let _y = &10;
        match (o) {
            Option::Some(n) if (_y == &5) => n,
            Option::None => 1,
            z => default(z),
        }
    }


}
