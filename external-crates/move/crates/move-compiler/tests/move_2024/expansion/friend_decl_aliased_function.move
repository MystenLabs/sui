address 0x42 {
module A {
    public fun a() {}
    public fun foo() {}
}

module M {
    use 0x42::A::a;
    use 0x42::A::foo;
    friend a;
    friend foo;

    public(friend) fun m() {
        a();
        foo();
    }
}
}
