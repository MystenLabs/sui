module 0x42::M {
    // Valid positional field struct declaration
    public struct Foo(u64) has copy, drop;

    fun Foo(_x: u64) { }

    fun x() {
        let _x = Foo(0);
        abort 0
    }
}
