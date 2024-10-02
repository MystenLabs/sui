module a::m {
    public macro fun do($f: || ()) { $f() }
    public fun q() { }
    public fun t() { do!(|| q() ) }
}
