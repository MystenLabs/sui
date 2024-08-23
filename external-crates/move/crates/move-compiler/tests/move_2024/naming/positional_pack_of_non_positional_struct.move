module 0x42::M {
    public struct Foo { field: u64 } has copy, drop;

    fun x() {
        // Error: Positional pack of non-positional struct
        let _x = Foo(0);
        abort 0
    }
}
