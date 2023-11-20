module 0x8675309::M {
    struct R {f: u64}
    struct S { g: u64 }

    fun t0() {
        let S { g } = R {f :0}; g;
        let (S { g }, R { f }) = (R{ f: 0 }, R{ f: 1 }); g; f;
    }

    fun t1() {
        let x = (); x;
        let () = 0;
        let (x, b, R{f}) = (0, false, R{f: 0}, R{f: 0}); x; b; f;
        let (x, b, R{f}) = (0, false); x; b; f;
    }

    fun t2() {
        let x: () = 0; x;
        let (): u64 = ();
        let (x, b, R{f}): (u64, bool, R, R) = (0, false, R{f: 0}); x; b; f;
        let (x, b, R{f}): (u64, bool) = (0, false, R{f: 0}); x; b; f;
    }
}
