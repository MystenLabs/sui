module a::m {
    macro fun foo<T>(x: u64, f: |u64|) {
        f(x)
    }
}
