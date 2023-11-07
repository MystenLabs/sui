module 0x42::M {
    fun t() {
        // Trailing underscore after bit-width suffix not allowed
        let _ = 0u8_;
    }
}
