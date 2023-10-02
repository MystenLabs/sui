module a::m {
    fun foo<T>(x: T) {
        use fun foo as T.foo;
        x.foo();
        abort 0
    }
}
