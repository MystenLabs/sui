module a::m {
    macro fun foo<$T>($x: $T): $T { $x }
    fun bar(): u64 { foo<u64>!(42) }
}
