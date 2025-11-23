module 0x8675309::M {
    fun t0(cond: bool) {
        if (cond) () else 0u64;
        if (cond) 0u64 else ();
    }

    fun t1(cond: bool) {
        if (cond) @0x0 else 0u64;
        if (cond) 0u64 else false;
    }

    fun t2(cond: bool) {
        if (cond) (0u64, false) else (1u64, 1u64);
        if (cond) (0u64, false) else (false, false);
        if (cond) (0u64, false) else (true, @0x0);
    }

    fun t3(cond: bool) {
        if (cond) (0u64, false, 0u64) else (0u64, false);
        if (cond) (0u64, false) else (0u64, false, 0u64);
    }

}
