module 0x8675309::M {
    struct R {f: u64}

    fun t0() {
        let _x: ();
        let (): u64;
        let (x, b, R{f}): (u64, bool, R, R); x; b; f;
        let (x, b, R{f}): (u64, bool); x; b; f;
    }
}
