module a::m {
    // macro args are call-by-name, not all usages are valid
    macro fun foo<$T>($x: $T) {
        copy $x;
        move $x;
    }

    fun t() {
        foo!(0);
    }
}
