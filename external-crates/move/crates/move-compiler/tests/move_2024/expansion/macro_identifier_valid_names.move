module a::m {
    // TODO should we allow all of these?
    macro fun foo<$T, $x>(
        _: u64,
        $_: u64,
        $f: |u64| -> u64,
        $X: bool,
        $u64: u8,
        $vector: address,
    ) {
        $f($_);
        $X;
        $u64;
        $vector;
    }
}
