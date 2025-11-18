module a::m {
    macro fun call($f: |u64| -> u64, $x: u64): u64 {
        $f = 0u64;
        $x = 0u64;
        $f($x)
    }

    fun t() {
        // ensure the macro is expanded
        call!(|_| false, 0) + 1;
    }
}
