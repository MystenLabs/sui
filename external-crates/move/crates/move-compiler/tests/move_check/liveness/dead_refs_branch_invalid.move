module 0x8675309::M {
    fun t0(cond: bool) {
        let x = 0u64;
        let x_ref = &mut x;
        if (cond) {
            *x_ref = 0;
        };
        _ = x;
        _ = move x;
        *x_ref = 0;
    }

    fun t1(cond: bool) {
        let x = 0u64;
        let x_ref = &mut x;
        if (cond) {
        } else {
            *x_ref = 0;
        };
        _ = x;
        _ = move x;
        _ = *x_ref;
    }

}
