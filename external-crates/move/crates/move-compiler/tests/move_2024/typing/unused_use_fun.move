// explicit unused
module a::m {
    fun foo(_: u64) {}

    use fun foo as u64.f;

    fun t() {
        use a::m::foo as f;
    }
}

// implicit unused method alias
module a::x {
    public struct X() has drop;
    public fun drop(_: X) {}
}

module b::other {
    use a::x::drop as f;

    fun t() {
        use a::x::drop as f;
    }
}
