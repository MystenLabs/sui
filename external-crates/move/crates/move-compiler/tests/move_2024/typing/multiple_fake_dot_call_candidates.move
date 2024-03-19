module a::m {
    public struct S {}
    public fun foo(_: &S) {}
}

module a::t1 {
    // this needs to be removed from the use global use fun list or the compiler will panic
    use a::m::foo as bar;

    public fun foo(s: &a::m::S) { s.bar() }
}

module a::t2 {
    // this needs to be removed from the use global use fun list or the compiler will panic
    use a::m::foo as bar;

    public fun foo(s: &a::m::S) { s.bar() }
}
