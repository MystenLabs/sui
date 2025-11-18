module 0x8675309::M {
    struct R {}

    fun t0(cond: bool) {
        if (cond) () else ();
    }

    fun t1(cond: bool) {
        if (cond) 0x0 else 0x0u64;
        if (cond) false else false;
        R {} = if (cond) R{} else R{};
        if (cond) &0u64 else &1;
        if (cond) &mut 0 else &mut 1u64;
    }

    fun t2(cond: bool) {
        if (cond) (0, false) else (1u64, true);
        (_, _, _, R{}) = if (cond) (0u64, 0x0, &0, R{}) else (1, 0x1u64, &1u64, R{});
    }

}
