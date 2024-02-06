module a::m {
    macro fun call<$T>($f: || -> $T): $T {
        $f()
    }

    fun t() {
        call!(|| -> () { });
        call!(|| -> () { () });
        call!(|| -> u64 { 0 });
        call!(|| -> (u64, u8) { (0, 0) });
    }
}
