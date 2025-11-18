module 0x8675309::M {
    struct R {
        f: u64,
        b: u8,
    }

    fun t0(x: u64, b: u8, r: R) {
        0u64 << 0;
        1u64 << 0;
        0u64 << 1;
        0u64 << (1: u8);
        (0: u8) + 1;
        (0: u128) << 1;
        (0u64) << (1);
        copy x << copy b;
        r.f << r.b;
        1u64 << r.b << r.b << 0;
        R {f: _, b: _} = r
    }
}
