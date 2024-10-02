// a param with a leading _ does not get unused mut ref warnings
module a::m {
    struct S has drop { f: u64 }

    public fun param(_x: &mut S): &S {
        let r: &S;
        freeze(_x);
        let _: &S = _x;
        let _: &u64 = &_x.f;
        ignore(_x);
        r = _x;
        r;
        _x
    }

    fun ignore<T>(_: &T) {}
}
