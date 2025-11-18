module 0x8675309::M {
    struct S {}

    fun t0() {
        let _x: &mut u64 = &0u64;
    }

    fun t1() {
        let (_x, _y): (&mut u64, &mut u64) = (&0u64, &0u64);
        let (_x, _y): (&mut u64, &u64) = (&0u64, &0u64);
        let (_x, _y): (&u64, &mut u64) = (&0u64, &0u64);
    }

}
