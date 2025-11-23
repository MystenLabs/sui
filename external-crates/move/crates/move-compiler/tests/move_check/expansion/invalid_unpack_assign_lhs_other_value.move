module 0x42::M {
    fun foo() {
        0u64 {} = 0;

        foo() = 0u64;

        foo().bar() = 0;
    }
}
