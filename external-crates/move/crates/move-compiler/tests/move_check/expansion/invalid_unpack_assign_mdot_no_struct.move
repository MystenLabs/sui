module 0x8675309::M {
    fun foo() {
        Self::f {} = 0u64;
        Self::f() = 0u64;
    }
}
