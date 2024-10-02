module 0x42::M {
    #[a, a(x = 0)]
    fun foo() {}

    #[b(a, a = 0, a(x = 1))]
    fun bar() {}
}
