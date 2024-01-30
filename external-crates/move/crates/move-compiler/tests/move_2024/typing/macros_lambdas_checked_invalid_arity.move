module a::m {
    macro fun foo<$T, $U>($f: |$T| -> $U, $g: |$T, $T| -> $U, $h: || -> ($U, $U)) {
        $f(0);
        $g(0, 1);
        $h();
    }

    fun t() {
        foo!<u64, vector<u8>>(
            || vector[], // invalid
            |a, b| vector[(a as u8), (b as u8)],
            || (b"hello", b"world"),
        );
        foo!<u64, vector<u8>>(
            |_, _| vector[], // invalid
            |a, b| vector[(a as u8), (b as u8)],
            || (b"hello", b"world"),
        );
        foo!<u64, vector<u8>>(
            |_| (vector<u8>[], vector<u8>[]), // invalid
            |a, b| vector[(a as u8), (b as u8)],
            || (b"hello", b"world"),
        );
        foo!<u64, vector<u8>>(
            |_| vector[],
            |a, b, _| vector[(a as u8), (b as u8)], // invalid
            || (b"hello", b"world"),
        );
        foo!<u64, vector<u8>>(
            |_| vector[],
            |a, b| vector[(a as u8), (b as u8)],
            || (b"hello", b"world", b"!"), // invalid
        );
        foo!<u64, vector<u8>>(
            |_| vector[],
            |a, b| vector[(a as u8), (b as u8)],
            || b"hello", // invalid
        );
    }
}
