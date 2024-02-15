// constants and functions do do not conflict since they cannot be used to start an access chain
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
    public fun all_foo(foo: foo::S) {
        foo(foo::new());
        foo(foo);
    }
}

module b::C {
    public struct S() has copy, drop;
    public fun new(): S { S() }
}

module b::m {
    use b::C;
    const C: u64 = 0;
    public fun eat(_: C::S): u64 {
        C::new();
        C
    }
}
