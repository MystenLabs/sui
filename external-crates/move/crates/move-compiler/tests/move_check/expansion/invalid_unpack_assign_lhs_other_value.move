module 0x42::M {
    fun foo() {
        ERROR
        {} = 0;

        foo() = 0;

        foo().bar() = 0;
    }
}
