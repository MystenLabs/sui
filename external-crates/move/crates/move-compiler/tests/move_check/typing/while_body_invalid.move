module 0x8675309::M {
    fun t0(cond: bool) {
        while (cond) 0u64;
        while (cond) false;
        while (cond) { @0x0 };
        while (cond) { let x = 0u64; x };
        while (cond) { if (cond) 1u64 else 0 };
    }
}
