// these usages mean the mutable reference is valid
module a::m {
    public fun assignment(x: &mut u64) {
        let i = 0;
        let r = &mut i;
        let r2 = copy r;
        *&mut 0 = 1;
        *x = 1;
        *r2 = 1;
        *r = 1;
    }

    public fun call(x: &mut u64) {
        let i = 0;
        let r = &mut i;
        let r2 = copy r;
        ignore(&mut 0);
        ignore(x);
        ignore(r2);
        ignore(r);
    }

    public fun ret(x: &mut u64): &mut u64 {
        x
    }

    public fun ignore<T>(_: &mut T) {}
}
