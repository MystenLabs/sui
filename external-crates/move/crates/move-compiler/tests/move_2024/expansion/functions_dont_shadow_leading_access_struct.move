// module member aliases do not shadow leading access names
module a::f {
    public struct f() has copy, drop;
    public fun foo() {}
}

module a::with_struct {
    fun f() {}
    fun x() {}

    fun t() {
        use a::f::f;
        {
            use a::with_struct::x as f;
            f::foo(); // resolves to struct
            f() // resolves to function
        }
    }
}
