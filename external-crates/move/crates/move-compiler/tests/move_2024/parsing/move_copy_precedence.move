module a::m {
    public struct S has copy, drop { f: u64 }
    fun test(x: u64, s: S) {
        copy x + 1;
        copy s.f + 1;
        &copy x;
        &copy s.f;

        let x1 = x;
        let x2 = x;
        let s1 = s;
        let s2 = s;
        move x1 + 1;
        move s1.f + 1;
        &move x2;
        &move s2.f;

        if (false) move x else copy x;
        if (false) copy s else move s;
    }
}
