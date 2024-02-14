module a::m {
    macro fun foo($f: |u64| -> u64, $x: u64) {
        $f(0);
        $x(0);
    }

    fun t() {
        // mismatch on parameters that are actually called
        foo!(0, |x| x);
        // call a non lambda
        foo!(|x| x, 0);
    }
}
