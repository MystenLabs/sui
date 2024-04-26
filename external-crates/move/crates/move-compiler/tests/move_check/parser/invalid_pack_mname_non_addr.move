module 0x42::M {
    struct S {}
    fun foo() {
        false::M::S { }
    }

    fun bar() {
        fun baz()::baz()::M::S { }
    }
}
