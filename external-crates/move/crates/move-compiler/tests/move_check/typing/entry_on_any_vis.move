module 0x2::M {
    // entry can go on any visibility
    entry fun f1() {}

    public entry fun f2() {}

    public(friend) entry fun f3() {}
}
