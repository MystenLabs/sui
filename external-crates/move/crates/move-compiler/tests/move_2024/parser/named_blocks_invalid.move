module 0x42::m {

    fun t0(cond: bool): u64 {
        name: {
            if (cond) { return 'name 10 };
            20
        }
    }

}
