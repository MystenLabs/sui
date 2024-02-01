module a::m {
    macro fun call<$T>($f: || -> $T): $T {
        $f()
    }

    fun t() {
        call!(|| -> u64 0);
    }
}
