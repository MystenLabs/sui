// warn on unused mutable reference, i.e. it could have been immutable
module a::m {
    public(friend) fun unused(x: &mut u64) {
        let i = 0;
        let r = &mut i;
        let r2 = copy r; // should point only to r
        &mut 0;
        x;
        r;
        r2;
    }

    public(friend) fun ret(x: &mut u64): &u64 {
        x
    }
}
