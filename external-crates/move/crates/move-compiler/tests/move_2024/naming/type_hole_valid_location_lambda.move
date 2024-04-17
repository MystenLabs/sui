module a::m {
    macro fun call<$T>($f: |$T| -> $T, $x: $T): $T {
        $f($x)
    }

    fun t() {
        call!<_>(|_| -> _ { 0 }, 1);
        call!<_>(|_: _| -> _ { 0 }, 1);
        call!<vector<_>>(|_: vector<_>| -> vector<_> { vector<_>[] }, vector[0]);
    }
}
