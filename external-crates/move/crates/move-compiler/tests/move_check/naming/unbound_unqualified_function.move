module 0x8675309::M {
    fun foo() {
        bar();
        let x = bar(); x;
        *bar() = 0;
    }
}
