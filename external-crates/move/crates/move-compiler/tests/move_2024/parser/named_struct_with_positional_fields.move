module 0x42::M {
    // Invalid positional field struct declaration
    public struct Foo{u64, u16} has copy, drop;
}
