module a::m {
    macro fun call($f: |u64| -> u64): u64 {
        $f(42)
    }

    fun t() {
        call!(|x| -> u64 'a: { return 'a x });
    }
}
