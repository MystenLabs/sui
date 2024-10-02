module 0x8675309::A {

    struct S has drop { f: u64 }

    fun four(): (u64, u64, u64, u64) {
        (0, 1, 2, 3)
    }

    public fun mixed() {
        let x: u64;
        let r = 0;
        let _r_ref = &mut r;
        let _s = S { f: 0 };
        (_, x, _, s.f) = four();
    }

}
