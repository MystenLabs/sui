module a::t1 {
    public struct S()

    public use fun foo as S.foo;
    fun foo(_: &S) {}
}

module a::t2 {
    public struct S()

    public use fun foo as S.bar;
    fun bar(_: &S) {}
    fun foo(_: &S) {}
}
