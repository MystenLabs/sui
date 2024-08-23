module a::m {
    public fun foo() {}
    public macro fun bar() { a::n::bar() }
}

module a::n {
    public fun bar() {}
    public macro fun foo() { a::m::foo() }
}

module a::t {
    public fun foo() { a::m::bar!() }
    public fun bar() { a::n::foo!() }
}
