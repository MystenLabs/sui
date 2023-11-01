module a::m {

    struct FBox {
        f: F,
    }

    struct F {
        f1: u8,
        f2: u16,
        f3: u64,
    }

    // f1 used mutably, so do not complain about f2 or f3
    public fun foo(f: &mut FBox): &mut u8 {
        let f = &mut f.f;
        let F { f1, f2: _, f3 } = f;
        assert!(*f3 >= 0, 42);
        f1
    }
}
