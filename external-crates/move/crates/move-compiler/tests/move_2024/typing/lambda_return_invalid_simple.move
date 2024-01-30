module a::m {
    macro fun call<$T>($f: || -> $T): $T {
        $f()
    }

    #[allow(dead_code)]
    fun simple() {
        call!<u64>(|| return &0);
        call!<&u64>(|| return 0);
        call!<(&u64, u8)>(|| return (&0, 1, 3));
    }

}
