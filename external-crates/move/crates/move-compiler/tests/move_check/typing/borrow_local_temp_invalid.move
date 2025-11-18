module 0x8675309::M {
    fun t0() {
        &();
        &(0u64, 1u64);
        &(0u64, 1u64, true, @0x0);
    }

    fun t1() {
        &(&0u64);
        &(&mut 1u64);
        &mut &2u64;
        &mut &mut 3u64;
    }
}
