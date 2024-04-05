module a::m {

    fun t(t: u64, match: u64): bool {
        t == match
    }

    fun t(t: u64, match: u64): bool {
        if (t == match) { true } else { false }
    }

}
