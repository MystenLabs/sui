module a::m {
    macro fun foo<$T>($x: $T): $T { $x }
    fun bar() { foo!; }
}
