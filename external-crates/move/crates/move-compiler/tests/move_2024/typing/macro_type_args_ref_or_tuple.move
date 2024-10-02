module a::m {
    macro fun foo<$T>() {}

    fun t() {
        // invalid for normal functions
        foo!<&u64>();
        foo!<&mut u64>();
        foo!<()>();
        foo!<(u64, bool)>();
    }
}
