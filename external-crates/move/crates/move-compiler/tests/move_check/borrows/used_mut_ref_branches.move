module a::m {
    public fun both_unused(cond: bool, x: &mut u64) {
        let i = 0;
        let r = if (cond) copy x else &mut i;
        *r = 0;
    }

    public fun one_unused(cond: bool, x: &mut u64) {
        let i = 0;
        *x = 0;
        let r = if (cond) copy x else &mut i;
        *r = 0;
    }
}
