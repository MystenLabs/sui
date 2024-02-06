module a::m {
    macro fun do<$T>($f: |$T| -> $T): $T {
        $f<u64>(0)
    }
}
