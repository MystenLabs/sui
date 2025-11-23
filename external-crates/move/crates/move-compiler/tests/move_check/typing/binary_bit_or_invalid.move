module 0x8675309::M {
    struct S has drop { u: u64 }
    struct R {
        f: u64
    }

    fun t0(r: R, s: S) {
        false | true;
        1u64 | false;
        false | 1u64;
        @0x0 | @0x1;
        (0: u8) | (1: u128);
        r | r;
        s | s;
        1u64 | false | @0x0 | 0u64;
        () | ();
        1u64 | ();
        (0u64, 1u64) | (0u64, 1u64, 2u64);
        (1u64, 2) | (0, 1u64);
    }
}
