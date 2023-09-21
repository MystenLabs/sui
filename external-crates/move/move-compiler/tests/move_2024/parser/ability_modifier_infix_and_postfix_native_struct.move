address 0x42 {
module M {
    // has both invalid declaration since postfix ability declarations
    // are not allowed for native structs
    public native struct Foo has copy, drop; has store;
}
}
