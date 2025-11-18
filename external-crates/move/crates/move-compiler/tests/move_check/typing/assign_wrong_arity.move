module 0x8675309::M {
    struct R {f: u64}

    fun t0() {
        let x;
        x = ();
        x = (0u64, 1u64, 2u64);
        () = 0u64;
        let b;
        let f;
        (x, b, R{f}) = (0u64, false, R{f: 0}, R{f: 0});
        (x, b, R{f}) = (0u64, false);
    }
}
