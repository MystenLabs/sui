// tests for cases that self-assignment should not warn

module a::m {
    const C: u64 = 112;

    fun t() {
        let x1 = 5;
        x1;
        x1 = 5; // we don't track values
        x1;

        let c1 = C;
        c1;
        c1 = C; // we don't track values
        c1;

        let x2 = 5;
        x2;
        let x2 = x2; // shadowing is not self-assignment
        x2;

        let (x3, x4) = (5, 5);
        x3;
        x4;
        (x4, x3) = (x3, x4); // swap is not self-assignment
        x3;
        x4;

        let r1 = &mut 0;
        let r2 = &mut 0;
        *r1;
        *r2;
        *r1 = *r2; // different references
        *r1;

        let r = &mut 0;
        *id(r) = *id(r);

        let x5 = 0;
        x5;
        x5 = { let x5 = 0; x5 }; // different x's
        x5;
    }


    struct S has copy, drop { f1: u64, f2: u64 }
    struct P has copy, drop { s1: S, s2: S }
    fun fields(m1: &mut S, m2: &mut S, s1: S, s2: S) {
        s1.f1 = s1.f2; // different fields
        m1.f1 = m1.f2; // different fields
        s1.f1 = s2.f1; // different locals
        m1.f1 = m2.f1; // different references
    }

    fun nested_fields(p1: &mut P, p2: &mut P) {
        p1.s1.f1 = p1.s1.f2; // different fields
        p1.s1.f1 = p2.s1.f1; // different references
    }

    fun id<T>(x: &mut T): &mut T {
        x
    }
}
