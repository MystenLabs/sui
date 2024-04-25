module a::foo {
    use a::foo;

    public fun foo() {}

    // could resolve to module or module member
    #[ext(attr = foo)]
    fun t1() {}

    public struct S()
    // does not resolve to module access
    #[ext(attr = S::x)]
    fun t2() {}
}
