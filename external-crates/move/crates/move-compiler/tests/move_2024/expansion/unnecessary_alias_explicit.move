module a::m {
    public struct S()

    public fun foo() {}
}

module a::t {
    use a::m::{Self, S, foo};

    public fun t() {
        use a::m; // unused and duplicate
        use a::m::S; // unused and duplicate
        use a::m::foo; // unused and duplicate
    }
}
