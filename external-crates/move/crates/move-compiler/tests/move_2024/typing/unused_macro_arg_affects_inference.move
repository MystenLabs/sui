module a::m {
    public struct X<phantom T: copy>() has copy, drop;
    public struct None()


    macro fun foo<$T, $U: copy>(
        _: X<$T>,
        _: $T,
    ) {
    }

    fun t() {
        // we cannot infer the type argument because the None() is not used
        X().foo!(None())
    }
}
