module prettier::pattern_matching {
    fun f(x: MyEnum): u8 {
        match (x) {
            MyEnum::Variant(1, true) => 1,
            MyEnum::Variant(_, _) => 1,
            MyEnum::OtherVariant(_, 3) => 2,
            // Now exhaustive since this will match all values of MyEnum::OtherVariant
            MyEnum::OtherVariant(..) => 2,
        }
    }

    fun match_pair_bool(x: Pair<bool>): u8 {
        match (x) {
            Pair(true, true) => 1,
            Pair(true, false) => 1,
            Pair(false, false) => 1,
            // Now exhaustive since this will match all values of Pair<bool>
            Pair(false, true) => 1,
        }
    }

    fun incr(x: &mut u64) {
        *x = *x + 1;
    }

    fun match_with_guard_incr(x: u64): u64 {
        match (x) {
            x => ({ incr(&mut x); x == 1 }),
            // ERROR:    ^^^ invalid borrow of immutable value
            _ => 2,
        }
    }

    fun match_with_guard_incr2(x: &mut u64): u64 {
        match (x) {
            x => ({ incr(&mut x); x == 1 }),
            // ERROR:    ^^^ invalid borrow of immutable value
            _ => 2,
        }
    }
}
