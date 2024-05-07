module a::m {
    fun foo(f: u64) {
        use fun f as u64.f;
        f;
    }

    macro fun bar(f: u64) {
        use fun f as u64.f;
        f;
    }

    macro fun baz($f: u64) {
        use fun $f as u64.f;
        $f;
    }
}
