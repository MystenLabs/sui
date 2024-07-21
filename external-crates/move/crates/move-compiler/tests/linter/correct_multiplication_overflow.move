module 0x42::M {
    public fun safe_multiplication() {
        let a: u64 = 10;
        let b: u64 = 20;
        let _ = a * b; // Should not trigger a warning
    }
}
