module 0x42::M {
    // has both invalid declaration since postfix ability declarations
    // are not allowed for native structs
    public native struct Foo has copy, drop; has store;
}
