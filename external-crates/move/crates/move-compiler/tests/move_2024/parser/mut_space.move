module a::m {
    public struct S {
        f: u64,
    }
    public fun foo(x: & mut S): & mut u64 {
        & mut x.f;
        &
        mut x.f
    }
}
