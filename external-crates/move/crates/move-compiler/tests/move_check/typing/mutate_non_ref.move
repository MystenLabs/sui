module 0x8675309::M {
    struct S has copy, drop { f: u64 }
    struct X has copy, drop { s: S }

    fun t0() {
        let u = 0u64;
        *u = 1u64;

        let s = S { f: 0 };
        *s = S { f: 0 };
        *s.f = 0u64;

        let s_ref = &mut S { f: 0 };
        *s_ref.f = 0u64;

        let x = X { s: *&s };
        *x.s = S { f: 0 };
        *x.s.f = 0u64;

        let x_ref = &mut X { s: *&s };
        *x_ref.s = S{ f: 0 };
        *x_ref.s.f = 0u64;

    }
}
