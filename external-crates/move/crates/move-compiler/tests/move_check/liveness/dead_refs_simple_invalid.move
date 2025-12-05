module 0x8675309::M {
    fun t0() {
        let x = 0;
        let x_ref = &mut x;
        _ = x;
        _ = move x;
        *x_ref = 0u64;
    }

    fun t1() {
        let x = 0u64;
        let x_ref = &mut x;
        _ = x;
        _ = move x;
        _ = *x_ref;
    }

}
