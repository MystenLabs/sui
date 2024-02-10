module a::m {
    fun f() {}
    // if we ever add non-$ var calls, we will need to fix this
    macro fun do<$T>(f: || -> $T): $T {
        f()
    }
}
