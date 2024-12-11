module a::m {
    public struct None()
    public struct NeedsCopy<phantom T: copy> {} has copy, drop, store;

    macro fun foo<$T: copy>() {}

    macro fun bar<$T>(_: NeedsCopy<$T>) {}

    macro fun baz<$T>(): NeedsCopy<$T> { abort 0 }

    #[allow(dead_code)]
    fun t() {
        foo!<None>();
        bar!<None>(NeedsCopy {});
    }

    fun t2() {
        baz!<None>()
    }

}
