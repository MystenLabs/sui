module a::m {
    macro fun foo($f: |u64, u64| -> u64) {
        $f();
        $f(0);
        $f(0, 1, 2);
    }

    fun t() {
        foo!(|x, y| x + y)
    }
}
