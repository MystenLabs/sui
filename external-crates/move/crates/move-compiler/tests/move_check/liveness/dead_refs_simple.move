module 0x8675309::M {
    fun t() {
        let x = 0u64;
        let x_ref = &mut x;
        *x_ref = 0;
        _ = x;
        _ = move x;
    }
}
