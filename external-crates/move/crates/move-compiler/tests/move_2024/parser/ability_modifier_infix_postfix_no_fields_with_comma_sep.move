module 0x42::M {
    // has both prefix and invalid postfix ability declarations
    public native struct Foo has copy, drop, has store;
}
