// Negating a bool through a variable should produce a clear error.
module 0x42::m {
    fun neg_bool_var() {
        let _b = true;
        let _x = -_b;
    }
}
