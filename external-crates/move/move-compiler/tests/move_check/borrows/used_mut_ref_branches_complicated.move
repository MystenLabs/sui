module a::m {
    struct S has copy, drop { f: u64, g: u64 }

    public fun t1(cond: bool, other: &mut S) {
        let s = S { f: 0, g: 0 };
        let f;
        if (cond) f = &mut s.f else f = &mut other.f;
        *f = 0;
        s = S { f: 0, g: 0 };
        s;
    }

    public fun t2(cond: bool, other: &mut S) {
        let s = S { f: 0, g: 0 };
        let f;
        if (cond) f = &mut s else f = other;
        *f = S { f: 0, g: 0 };
        s = S { f: 0, g: 0 };
        s;
    }

    public fun t3(cond: bool, other: &mut S) {
        let s = S { f: 0, g: 0 };
        let f;
        if (cond) f = id_mut(&mut s) else f = other;
        *f = S { f: 0, g: 0 };
        s = S { f: 0, g: 0 };
        s;
    }

    public fun id_mut<T>(x: &mut T): &mut T { x }

}
