module a::m {
    public struct X() has copy, drop, store;

    macro fun foo(
        $x: &u64,
        $p: X,
        $s: &mut X,
        $f: |u64, (&u64, X, &mut X)|
    ) {
        $f(0, ($x, $p, $s))
    }

    fun t() {
        let x1 = X();
        foo!(
            &0,
            x1,
            &mut X(),
            |_: u64, (_, X(), _x): (&u64, X, &mut X)| ()
        );
        foo!(
            &0,
            x1,
            &mut X(),
            |_, (_, X(), _x): (&u64, X, &mut X)| ()
        );
        foo!(
            &0,
            x1,
            &mut X(),
            |_: u64, (_, X(), _x)| ()
        )
    }
}
