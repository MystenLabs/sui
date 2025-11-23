module 0x8675309::M {
    fun f() {
        let (x1, x2,) = (1u64, 2u64); // Test a trailing comma in the let binding
        x1;
        x2;
    }
}
