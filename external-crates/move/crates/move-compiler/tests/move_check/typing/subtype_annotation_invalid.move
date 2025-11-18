module 0x8675309::M {
    struct S {}

    fun t0() {
        (&0u64: &mut u64);
    }

    fun t1() {
        ((&0u64, &0u64): (&mut u64, &mut u64));
        ((&0u64, &0u64): (&mut u64, &u64));
        ((&0u64, &0u64): (&u64, &mut u64));
    }

}
