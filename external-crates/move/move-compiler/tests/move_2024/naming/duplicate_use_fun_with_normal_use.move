module a::m {
    public struct X has copy, drop, store {}

    public fun foobar(_: &X) {}
}

module a::n1 {
    use a::m::{X, foobar, foobar as foobaz};
    use fun foobar as X.foobaz;

    fun dispatch(x: &X) {
        x.foobaz();
    }
}

module a::n2 {

    fun confusing(x: &a::m::X) {
        use a::m::{X, foobar, foobar as foobaz};
        use fun foobar as X.foobaz;
        x.foobaz();
        foobar(x);
    }
}
