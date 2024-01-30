module a::m {
    macro fun foo<$T, $U>($f: |$T| -> $U, $g: |$T, $T| -> $U, $h: || -> $U) {
        $f(0);
        $g(0, 1);
        $h();
    }

    fun t() {
        foo!<u64, vector<u8>>(
            |_| vector[],
            |a, b| vector[(a as u8), (b as u8)],
            || b"hello",
        )
    }
}
