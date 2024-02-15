module a::foo {
    public struct S() has copy, drop;
    public fun new(): S { S() }
}

module a::m {
    use a::foo;
    public fun foo(_: foo::S) {}
}

module a::n {
    use a::foo;
    use a::m::foo;
}
