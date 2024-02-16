module a::m {
    public macro fun do($f: || ()) { $f() }
}
