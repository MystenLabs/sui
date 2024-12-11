// tests for cases that self-assignment should warn

module a::m {
    fun variables(p: u64) {
        p = p; // warn
        p;

        let x = 0;
        x;
        x = x; // warn
        x;

        x = move x; // warn
        x;
        x = copy x; // warn

        let other;
        (p, other, x) = (p, 0, x); // warn x2
        p;
        x;
        other;
    }

    struct S has copy, drop { f1: u64, f2: u64 }

    fun fields(m: &mut S, s: S) {
        *&mut m.f1 = m.f1;
        m.f1 =  *&mut m.f1;
        m.f1 =  *&m.f1;
        *&mut m.f1 =  *&m.f1;
        *&mut m.f1 =  *&mut m.f1;

        *&mut s.f1 = s.f1;
        s.f1 =  *&mut s.f1;
        s.f1 =  *&s.f1;
        *&mut s.f1 =  *&s.f1;
        *&mut s.f1 =  *&mut s.f1;
    }

    struct P has copy, drop { s1: S, s2: S }

    fun nested_fields(p: &mut P) {
        p.s1.f1 = p.s1.f1;
    }

    fun references(r: &mut u64) {
        *r = *r;
        *r;
        *copy r = *r;
        *move r = *copy r;

        let x = 0;
        *&mut x = *&mut x;
    }
}
