module a::m {
    macro fun do<$T>($f: || -> $T): $T { $f() }

    // simple test of break/return in a lambda
    fun t() {
        do!(|| {
            if (false) return 0;
            0
        });
    }
}
