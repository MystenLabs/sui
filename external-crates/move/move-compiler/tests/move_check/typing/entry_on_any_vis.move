module 0x2::M {
    // entry can go on any visibility
    entry fun f1() {}
    entry public fun f2() {}
    entry public(friend) fun f3() {}
}
