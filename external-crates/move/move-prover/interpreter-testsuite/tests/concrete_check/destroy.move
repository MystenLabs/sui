module 0x2::A {
    struct S {
        f1: bool,
        f2: u64,
    }

    fun foo(s: &S): u64 {
        s.f2
    }

    #[test]
    public fun destroy(): S {
        let s = S { f1: true, f2: 42 };
        let p = &s;
        let _ = p;
        s
    }
}
