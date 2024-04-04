module a::m {
    public struct S()
    public fun foo() {
        use a::m::S; // unused and duplciate
        use a::m::foo; // unused and duplciate
    }
}
