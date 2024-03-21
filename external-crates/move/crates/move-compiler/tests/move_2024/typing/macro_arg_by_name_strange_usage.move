module a::m {
    public struct X() has drop;
    public struct S { f: X } has drop;

    // some usages of the expression won't do what they would do if they werent used as a value
    // In other words, we don't re-interpret the expression as a pth
    macro fun foo<$T>($x: $T) {
        let mut x = $x;
        &x;
        &mut x;
    }

    fun t() {
        let x = X();
        foo!(x); // TODO improve this error message

        let s = S { f: X() };
        foo!(s.f); // TODO improve this error message
    }
}
