module a::m {
    // macro args are call-by-name, not all usages are valid
    macro fun foo<$T>($_x: $T) {
        $_x = 0;
    }

    fun t() {
        let x = 0;
        foo!(*x);
    }
}
