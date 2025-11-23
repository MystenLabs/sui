module 0x8675309::M {
    fun t0(cond: bool) {
        let v = 0u64;
        let x;
        let y;
        if (move cond) {
            x = &v;
            y = copy x;
        } else {
            y = &v;
            x = copy y;
        };
        move x;
        move y;
    }
}
