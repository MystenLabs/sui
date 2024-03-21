module a::m {
    public struct X() has copy, drop;
    macro fun call_foo<$T>($x: $T) {
        let x= $x;
        x.foo()
    }

    fun t() {
        call_foo!(X());
    }
}
