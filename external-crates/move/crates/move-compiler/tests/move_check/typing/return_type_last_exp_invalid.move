module 0x8675309::M {
    struct R {}

    fun t0(): u64 {
        ()
    }

    fun t1(): () {
        0u64
    }

    fun t2(): (u64, bool) {
        (0u64, false, R{})
    }

    fun t3(): (u64, bool, R, bool) {
        (0u64, false, R{})
    }

    fun t4(): (bool, u64, R) {
        (0u64, false, R{})
    }
}
