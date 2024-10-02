module 0x42::M {
    // Invalid positional field struct declaration
    public struct Foo(f: u64) has copy, drop;
}
