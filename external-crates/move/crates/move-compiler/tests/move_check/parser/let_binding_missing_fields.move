module 0x42::M {
    struct Generic<T: drop> has drop {
        g: T
    }
    fun g(g: Generic<u64>) {
        let Generic<u64> = g; // Test a type name with no field bindings
    }
}
