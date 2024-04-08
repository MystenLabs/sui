module a::m {

    fun t1(t: u64, match: u64): bool {
        t == match
    }

    fun t2(t: u64, match: u64): bool {
        if (t == match) { true } else { false }
    }

}
