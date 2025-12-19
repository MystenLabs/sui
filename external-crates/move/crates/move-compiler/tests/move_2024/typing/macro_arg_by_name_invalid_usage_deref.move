module a::m {
    // macro args are call-by-name, not all usages are valid
    macro fun foo<$T>($_x: $T) {
        $_x = 0u64;
    }

    fun t() {
        let x = 0u64;
        foo!(*x);
    }
}
