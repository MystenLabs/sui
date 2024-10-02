module a::m {
    macro fun foo<$T, $U>(
        $x: u64,
        $y: u64,
        $z: &mut u64
    ): (u64, $T, $U) {
        ($x, $y, $z)
    }

    macro fun ref<$T, $U>($f: || -> $T): &$U {
        $f()
    }

    macro fun double<$T, $U>($f: || -> $T): ($U, $U) {
        $f()
    }

    fun t() {
       foo!<u8, &mut u8>(1, 2u64, &mut 3u64);
       ref!<u64, u64>(|| &1);
       double!<u64, u64>(|| (0, 0));
       double!<u64, u64>(|| 0);
       double!<(u64, u64), u64>(|| (&0, &0));
    }
}
