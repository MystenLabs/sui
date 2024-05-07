module a::m {
    public struct X() has copy, drop;

    macro fun foo($x: X) {}

    fun t() {
        X().foo!()
    }

}
