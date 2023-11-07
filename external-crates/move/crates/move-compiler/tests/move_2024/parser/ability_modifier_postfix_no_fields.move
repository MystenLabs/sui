address 0x42 {
module M {
    // native structs cannot have suffix ability declarations.
    public native struct Foo has copy, drop has store;
}
}
