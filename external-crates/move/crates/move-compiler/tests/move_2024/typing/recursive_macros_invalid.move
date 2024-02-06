module a::m {
    // invalid cycle
    macro fun self_cyle(): u64 {
        1 + self_cyle!()
    }

    // invalid cycle of more than 1 node
    macro fun cycle1(): u64 {
        cycle2!()
    }

    macro fun cycle2(): u64 {
        cycle3!()
    }

    macro fun cycle3(): u64 {
        cycle1!()
    }

    // invalid cycle through lambda
    macro fun cycle_app($f: || -> u64): u64 {
        apply!(|| cycle_app!(|| $f()))
    }

    macro fun apply($f: || -> u64): u64 {
        $f()
    }

    // invalid cycle through by-name arg
    macro fun cycle_by_name($f: u64): u64 {
        by_name!(cycle_by_name!($f))
    }

    macro fun by_name($f: u64): u64 {
        $f
    }

    fun t() {
        self_cyle!();
        cycle1!();
        cycle_app!(|| 1);
        cycle_by_name!(1);
    }
}
