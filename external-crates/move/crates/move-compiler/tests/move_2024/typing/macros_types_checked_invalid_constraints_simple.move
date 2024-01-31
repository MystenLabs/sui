module a::m {
    public struct None()
    public struct NeedsCopy<phantom T: copy> {} has copy, drop, store;

    macro fun foo<$T: copy>() {}

    macro fun bar<$T>(_: NeedsCopy<$T>) {}

    macro fun baz<$T>(): NeedsCopy<$T> { abort 0 }

    fun t() {
        foo!<None>();
        bar!<None>(NeedsCopy {});
        baz!<None>(); // TODO do not complain about dead code?
    }
}
