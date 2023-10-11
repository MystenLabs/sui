// warn on unused mutable reference, i.e. it could have been immutable
module a::m {
    public fun t(x: &mut u64) {
        let i = 0;
        let r = &mut i;
        let r2 = copy r; // shoudl point only to r
        &mut 0;
        x;
        r;
        r2;
    }
}
