module a::m {
    public struct None() has drop;
    public struct X<phantom T: copy>() has copy, drop, store;
    public fun f<T: copy>(_: X<T>) {}

    macro fun apply<$T>($x: $T, $l: |$T|) {
        $l($x);
    }

    macro fun useless<$U>($x: X<$U>): X<$U> {
        let x = $x;
        freeze<X<$U>>(&mut X());
        f<$U>(X());
        X<$U>();
        x.f<$U>();
        apply!(x, |_: X<$U>| ());
        X<$U>() = x;
        let _: X<$U> = x;
        let X<$U>() = x;
        (0 as $U);
        (x: X<$U>);
        x
    }

    fun t() {
        // the susbstitutions are all correct, but should all hit errors
        useless!<None>(X());
    }


}
