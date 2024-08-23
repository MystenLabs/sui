module a::m {

    struct A has copy, drop {
        x: u64
    }

    struct B has copy, drop {
        a: A
    }

    public fun foo() {
        let _s = B { a: A { x: 0 } };
        let _tmp1 = _s.;
        let _tmp2 = _s.a.;
    }
}
