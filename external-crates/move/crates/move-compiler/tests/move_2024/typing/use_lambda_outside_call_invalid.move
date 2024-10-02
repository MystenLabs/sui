module a::m {
    macro fun foo($f: || -> u64) {
        let x = $f;
        x();
    }

    macro fun bar(mut f: || -> u64) {
        f = || 0;
    }

    macro fun baz($f: || -> || -> u64): || -> u64 {
        $f()
    }

    fun t() {
        foo!(|| 0);
        bar!(|| 0);
        foo!(baz!(|| || 0));
    }
}
