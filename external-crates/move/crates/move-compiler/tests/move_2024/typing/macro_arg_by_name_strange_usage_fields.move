module a::m {
    public struct X() has drop;
    public struct S { f: X } has drop;

    // some usages of the expression won't do what they would do if they werent used as a value
    // In other words, we don't re-interpret the expression as a pth
    macro fun foo<$T>($s: $T) {
        &$s.f;
        &mut $s.f;
    }

    fun t() {
        let s = S { f: X() };
        foo!(s); // TODO improve this error message
    }
}
