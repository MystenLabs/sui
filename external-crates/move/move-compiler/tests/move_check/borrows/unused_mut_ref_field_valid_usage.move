// these usages mean the mutable reference is valid
module a::m {
    struct S has drop { f: u64 }

    public fun assignment(param: &mut S) {
        let s = S { f: 0 };
        let r = &mut s;
        let param_f = &mut param.f;
        let r_f = &mut r.f;
        *&mut S { f: 0 }.f = 1;
        *param_f = 1;
        *r_f = 1;
    }

    public fun call(param: &mut S) {
        let s = S { f: 0 };
        let r = &mut s;
        let param_f = &mut param.f;
        let r_f = &mut r.f;
        ignore(&mut S { f: 0 }.f);
        ignore(param_f);
        ignore(r_f);
    }

    public fun ret(param: &mut S): &mut u64 {
        &mut param.f
    }

    public fun ignore<T>(_: &mut T) {}

}
