module 0x42::M {
    // Valid positional field struct declaration
    public struct Foo(u64) has copy, drop;
    public struct Bar has copy (u64) 
    public struct Baz()
    public struct Qux(
        /// A field with a doc comment
        u64,
        /** another field with another doc comment **/
        u64,)
}
