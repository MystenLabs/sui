module 0x42::M {
    fun f(_v: u64) {
        // Test an "if" expression missing parenthesis around the condition
        if v < 3 ()
    }
}
