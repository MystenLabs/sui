module a::m {
    macro fun call<$T>($f: || -> $T): $T {
        $f()
    }

    fun t() {
        // cannot annotate a lambda this way
        call!((|| 1 : || -> u64))
    }
}
