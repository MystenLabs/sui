module a::m {
    public struct X has copy, drop, store {}

    public use fun foobar as X.foobaz;
    public fun foobar(_: &X) {}

    fun foobaz(_: &X, _: u64) {}

    fun dispatch(x: &X) {
        x.foobaz();
    }
}
