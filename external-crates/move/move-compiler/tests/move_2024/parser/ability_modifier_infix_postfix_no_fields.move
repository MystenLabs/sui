address 0x42 {
module M {
    // has both prefix and invalid postfix ability declarations
    public native struct Foo has copy, drop has store;
}
}
