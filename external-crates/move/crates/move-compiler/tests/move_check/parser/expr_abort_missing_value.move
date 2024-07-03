module 0x42::M {
    fun f(_v: u64) {
        // Aborts always require a value
        if (v > 100) abort
    }
}
