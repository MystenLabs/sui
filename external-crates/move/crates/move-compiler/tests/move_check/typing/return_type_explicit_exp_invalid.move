module 0x8675309::M {
    struct R {}

    fun t0(): u64 {
        return ()
    }

    fun t1(): () {
        if (true) return 1u64 else return 0u64
    }

    fun t2(): (u64, bool) {
        loop return (0u64, false, R{});
        abort 0
    }

    fun t3(): (u64, bool, R, bool) {
        while (true) return (0u64, false, R{});
        abort 0
    }

    fun t4(): (bool, u64, R) {
        while (false) return (0u64, false, R{});
        abort 0
    }
}
