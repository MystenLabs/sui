module a::m {
    fun f() {}
    macro fun do<T>(f: || T): T {
        // TODO the local f should shadow the outer f
        f()
    }
}
