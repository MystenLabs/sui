module 0x8675309::M {
    fun foo<T, U>(_: T, _: U) {
    }

    fun t1() {
        foo<u64, u64>(false, false);
        foo<bool, bool>(0, false);
        foo<bool, bool>(false, 0);
        foo<bool, bool>(0, 0);
    }

    fun t2<T, U, V>(t: T, u: U, v: V) {
        foo<U, u64>(t, 0);
        foo<V, T>(u, v);
    }

}
