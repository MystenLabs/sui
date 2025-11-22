module a::foo {
    public fun goof() { }
}

module a::m {
    use a::foo;
    // Should suggest foo in pace of qoo
    public fun call() { qoo::goof() }
}
