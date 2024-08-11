module a::m {
    #[deprecated]
    public struct S()

    #[deprecated]
    public fun foo() {}
}

#[allow(deprecated_usage)]
module b::m1 {
    fun t<T>() {}
    public fun foo() {
        a::m::foo();
    }

    public fun s() {
        t<a::m::S>();
    }
}

module b::m2 {
    fun t<T>() {}

    #[allow(deprecated_usage)]
    public fun foo() {
        a::m::foo();
    }

    #[allow(deprecated_usage)]
    public fun s() {
        t<a::m::S>();
    }
}
