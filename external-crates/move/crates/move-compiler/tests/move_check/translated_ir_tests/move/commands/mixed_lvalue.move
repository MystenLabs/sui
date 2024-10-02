module 0x8675309::A {

    struct S has drop { f: u64 }

    fun four(): (u64, u64, u64, u64) {
        (0, 1, 2, 3)
    }

    public fun mixed() {
        let r = 0;
        let r_ref = &mut r;
        let s = S { f: 0 };

        (_, _, _, s.f) = four();
        (_, _, _, *r_ref) = four();
    }
}
