module 0x8675309::M {
    struct R {}
    fun t0() {
        ({ 0u64 } : bool);
        ({ &0u64 } : u64);
        ({ &mut 0u64 } : ());
        ({ R {} } : R);
        ({ (0u64, false, false) } : (u64, bool));
    }
}
