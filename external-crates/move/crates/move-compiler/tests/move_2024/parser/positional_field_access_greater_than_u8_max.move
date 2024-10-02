module 0x42::M {
    public struct Foo(u64)

    fun x(y: Foo): u64 {
        y.256
    }
}
