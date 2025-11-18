module a::m {
    public fun used(cond: bool) {
        let i = 0u64;
        let j = 0;
        let r = &mut i;
        while (cond) {
            *r = 1;
            r = &mut j;
        }
    }
}
