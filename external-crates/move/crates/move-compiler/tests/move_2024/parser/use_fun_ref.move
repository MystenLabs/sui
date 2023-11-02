module a::m {
    public struct X {}
    use fun foo as &X.foo;
    fun foo(_: &X) { abort 0 }
}
