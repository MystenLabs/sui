module a::m {
    // macro args are call-by-name, not all usages are valid
    macro fun foo<$T>($x: $T): $T {
        copy $x;
        move $x;
        $x
    }

    fun t() {
        let x = 0;
        foo!(x);
        x;
    }
}
