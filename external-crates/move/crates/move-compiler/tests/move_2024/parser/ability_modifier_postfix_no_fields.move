module 0x42::M {
    // native structs cannot have suffix ability declarations.
    public native struct Foo has copy, drop has store;
}
