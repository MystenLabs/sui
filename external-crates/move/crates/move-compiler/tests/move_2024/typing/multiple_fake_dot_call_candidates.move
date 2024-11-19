module a::m {
    public struct S {}

    public fun foo(_: &S) {}
}

module a::t1 {
    use a::m::foo as bar;

    public fun foo(s: &a::m::S) { s.bar() }
}

module a::t2 {
    use a::m::foo as bar;

    public fun foo(s: &a::m::S) { s.bar() }
}
