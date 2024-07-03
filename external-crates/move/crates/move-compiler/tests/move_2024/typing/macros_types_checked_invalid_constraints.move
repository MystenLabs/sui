module a::m {
    public struct None()
    public struct NeedsCopy<phantom T: copy> {} has copy, drop, store;

    macro fun foo<$T: copy, $U>(
        $_a: u64,
        $_r: &mut u64,
        $_n: NeedsCopy<$T>,
    ) {
        let _: NeedsCopy<$U> = NeedsCopy {};
    }

    macro fun ret<$T>(): $T {
        NeedsCopy {}
    }

    macro fun ret2<$T>(): NeedsCopy<$T> {
        NeedsCopy {}
    }

    #[allow(dead_code)]
    fun t() {
        // type args don't satisify constraints
        foo!<None, NeedsCopy<bool>>(0, &mut 1, NeedsCopy {});
        foo!<u64, NeedsCopy<None>>(0, &mut 1, NeedsCopy {});
        foo!<u64, None>(0, &mut 1, NeedsCopy {});
        ret!<None>();
        ret2!<None>();
    }
}
