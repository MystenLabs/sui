module 0x8675309::M {
    struct R {f: u64}

    fun t0() {
        let x: () = (); x;
        let (): u64 = 0;
        let (x, b, R{f}): (u64, bool, R, R) = (0, false, R{f: 0}, R{f: 0}); x; b; f;
        let (x, b, R{f}): (u64, bool) = (0, false); x; b; f;
    }
}
