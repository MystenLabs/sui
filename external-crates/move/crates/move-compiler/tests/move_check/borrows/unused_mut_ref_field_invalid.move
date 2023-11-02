// warn on unused mutable reference, i.e. it could have been immutable
// In these cases, the mutable reference is "used" but the extensions (fields) are not
module a::m {
    struct S has drop { f: u64 }

    public fun t(param: &mut S) {
        let s = S { f: 0 };
        let r = &mut s;
        let param_f = &mut param.f;
        let r_f = &mut r.f;
        &mut S { f: 0 }.f;
        param_f;
        r_f;
    }
}
