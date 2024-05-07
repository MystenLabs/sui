module a::m {
    public struct X<phantom T: copy>() has copy, drop;
    public struct None() has drop;

    fun mycopy<T: copy>(t: &T): T {
        *t
    }

    macro fun needs_copy<$T, $U, $V>(_: X<$T>, _: $U, $v: $V): X<$U> {
        let v = $v;
        mycopy(&v);
        X()
    }

    #[allow(dead_code)]
    fun t() {
        needs_copy!<None, None, None>(X(), None(), None());
    }
}
