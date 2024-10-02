// module member aliases do not shadow leading access names
module a::f {
    public fun foo() {}
}

module a::with_module {
    fun f() {}
    fun x() {}

    fun t() {
        use a::f;
        f::foo(); // resolves to module
        f() // resolves to function
    }

    fun t2() {
        use a::f;
        {
            use a::with_module::x as f;
            f::foo(); // resolves to module
            f() // resolves to function
        }
    }
}
