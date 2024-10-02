module a::m {
    public struct X() has copy, drop;

    fun foo(_: X) {}

    fun any<T>(): T { abort 0}

    macro fun call<$T>($f: || -> $T): $T {
        $f()
    }

    fun t() {
        // we must persist the return type, otherwise we will not know which foo to call
        call!(|| -> X { any() }).foo();
        call!(|| -> &u64 { &mut 0 });
    }
}
