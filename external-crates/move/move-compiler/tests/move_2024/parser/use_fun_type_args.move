module a::m {
    public struct X<phantom T> {}
    use fun foo as X<u64>.foo;
    fun foo<T>(_: X<T>) { abort 0 }
}
