module 0x42::M {
    fun foo() {
        0 {} = 0;

        foo() = 0;

        foo().bar() = 0;
    }
}
