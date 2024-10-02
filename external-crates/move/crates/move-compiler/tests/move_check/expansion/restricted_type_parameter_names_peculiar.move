#[allow(unused_type_parameter)]
module a::m {
    // none of these are invlaid ... but they are weird
    struct S<
        __,
        _u64,
        _T,
        x,
    > {}
    fun foo_<
        __,
        _u64,
        _T,
        x,
    >() {}
}
