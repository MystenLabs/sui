module a::T {
    public struct S() has copy, drop;
    public fun foo() {}
}

module a::m {
    use a::T;
    public fun t<T>(_: T::S): T::S {
        T::foo();
        T::S()
    }
}
