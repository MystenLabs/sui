module a::n {
    public fun goof() {}
}

module a::m {
    use a::n as n;
    // Should suggest 'n' instead of 'N'
    public fun call() { N::goof() }
}
