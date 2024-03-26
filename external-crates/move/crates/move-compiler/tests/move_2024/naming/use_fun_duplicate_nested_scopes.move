module a::m {
    public struct S()

    public fun foo(_: &S) {}
}

module a::t1 {
    use a::m::S;

    use fun a::m::foo as S.foo;
    fun call_foo(s: &S) {
        s.foo();
    }
}

module a::t2 {
    use a::m::S;

    use fun a::m::foo as S.bar;
    fun call_bar(s: &S) {
        use fun a::m::foo as S.bar;
        s.bar();
    }
}

module a::t3 {
    use a::m::S;

    use a::m::foo as bar;
    fun call_bar(s: &S) {
        use fun a::m::foo as S.bar;
        s.bar();
    }
}
