// public+public
// visibility should not affect these tests, but we are being exhausive
// for this first test
module a::m {
    public struct X has copy, drop, store {}

    public use fun foobar as X.f;
    public fun foobar(_: &X) {}

    public use fun foobaz as X.f;
    public fun foobaz(_: &X, _: u64) {}

    public fun dispatch(x: &X) {
        x.f();
    }
}

// public+internal
module a::m2 {
    public struct X has copy, drop, store {}

    public use fun foobar as X.f;
    public fun foobar(_: &X) {}

    use fun foobaz as X.f;
    fun foobaz(_: &X, _: u64) {}

    public fun dispatch(x: &X) {
        x.f();
    }
}

// internal+internal
module a::m3 {
    public struct X has copy, drop, store {}

    use fun foobar as X.f;
    public fun foobar(_: &X) {}

    use fun foobaz as X.f;
    fun foobaz(_: &X, _: u64) {}

    public fun dispatch(x: &X) {
        x.f();
    }
}
