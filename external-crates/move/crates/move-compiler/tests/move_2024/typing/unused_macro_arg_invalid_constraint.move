module a::m {
    public struct X<phantom T: copy>() has copy, drop;
    public struct None()


    macro fun needs_copy<$T, $U: copy>(
        _: $T,
        _: $U,
        $_t: $T,
        $_u: $U,
    ) {
    }

    fun t() {
        // these would all give constraint errors if they were used
        needs_copy!(X<None>(), None(), X<None>(), None());
    }
}
