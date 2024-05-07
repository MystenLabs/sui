module a::m {
    public struct X() has copy, drop;
    public fun u(_: &X): u64 { 0 }
    public fun b(_: &X): bool { false }
}

module b::other {
    use a::m::X;

    use fun a::m::u as X.f;

    fun example(x: &X) {
        let _: u64 = x.f();
        {
            use a::m::b as f;
            let _: bool = x.f();
        };
        let _: u64 = x.f();
        {
            use fun a::m::b as X.f;
            let _: bool = x.f();
        }
    }
}
