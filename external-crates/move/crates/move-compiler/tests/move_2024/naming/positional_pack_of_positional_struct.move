module 0x42::M {
    public struct Foo(u64) has copy, drop;

    fun x() {
        let _x = Foo(0);
        abort 0
    }
}
