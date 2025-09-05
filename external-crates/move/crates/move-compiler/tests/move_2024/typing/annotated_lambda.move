module a::m {
    macro fun call<$T>($f: || -> $T): $T {
        $f()
    }

    fun t() {
        // this sort of annotation is now supported
        call!((|| 1 : || -> u64));
    }
}
