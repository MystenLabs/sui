module a::m {
    public struct X() has copy, drop;

    fun foo(_: X) {}

    fun any<T>(): T { abort 0}

    macro fun call<$T>($f: || -> $T): $T {
        $f()
    }

    fun t() {
        call!(|| -> X { 0 }).foo();
        call!(|| -> &mut u64 { &0 });
    }
}
