module a::m {
    public struct X() has drop;
    public struct S { f: X } has drop;

    // this is all technically correct due to the binding. If we allowed `&$s.f` this would break,
    // but we explicitly prevent this.
    macro fun foo<$T>($s: $T) {
        let mut s = $s;
        &s.f;
        &mut s.f;
    }

    fun t() {
        let s = S { f: X() };
        foo!(s);
    }
}
