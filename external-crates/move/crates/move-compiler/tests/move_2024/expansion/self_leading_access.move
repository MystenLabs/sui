// module member aliases do not shadow leading access names
module a::foo {
    public struct S()
    public fun foo(): S {
        // TOOD fix this, should resolve to a::m::S
        foo::S()
    }
}
