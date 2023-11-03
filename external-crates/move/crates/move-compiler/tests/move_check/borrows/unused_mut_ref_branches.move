// unused mutable references in combination with ifs
module a::m {
    public fun both_unused(cond: bool, x: &mut u64) {
        let i = 0;
        if (cond) copy x else &mut i;
    }

    public fun one_unused(cond: bool, x: &mut u64) {
        let i = 0;
        *x = 0;
        if (cond) copy x else &mut i;
    }
}
