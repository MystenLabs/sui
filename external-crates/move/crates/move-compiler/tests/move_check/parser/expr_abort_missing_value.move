module 0x42::M {
    fun f(v: u64) {
        // Aborts always require a value if not in Move 2024
        if (v > 100) abort
    }
}
