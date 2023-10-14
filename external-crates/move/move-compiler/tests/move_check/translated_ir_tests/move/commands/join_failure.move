module 0x8675309::M {
    struct R { f:bool }
    fun t0(cond: bool) {
        let r = R{ f: false };
        let f;

        if (cond) {
            R{ f } = move r;
        } else {
            R{ f } = move r;
            r = R{ f: false };
        };
        R{ f: _ } = move r;
        f;
    }
}
