address 0x2 {

module X {
    struct S {}

    public fun foo(_: S) {
    }

    public fun bar(_: u64) {
    }
}

module M {
    use 0x2::X;
    struct S {}

    public fun foo(_: S) {
    }

    public fun bar(_: u64) {
    }

    fun t0() {
        foo(0u64);
        bar(S{});
        bar(@0x0);
    }

    fun t1() {
        X::foo(S{});
        X::foo(0u64);
        X::bar(S{});
        X::bar(false);
    }

}

}
