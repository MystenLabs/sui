// Unsuffixed negative literals fit into their inferred signed type, including MIN.
module 0x42::m {
    fun min_i8()   { let _x: i8   = -128; }
    fun min_i16()  { let _x: i16  = -32768; }
    fun min_i32()  { let _x: i32  = -2147483648; }
    fun min_i64()  { let _x: i64  = -9223372036854775808; }
    fun min_i128() { let _x: i128 = -170141183460469231731687303715884105728; }
    fun min_i256() { let _x: i256 = -57896044618658097711785492504343953926634992332820282019728792003956564819968; }

    fun small_neg_i8()  { let _x: i8  = -1; }
    fun small_neg_i16() { let _x: i16 = -1; }
    fun mid_neg_i32()   { let _x: i32 = -100000; }
    fun max_neg_i64()   { let _x: i64 = -9223372036854775807; }

    // Zero is the same value either way.
    fun neg_zero_i8()   { let _x: i8 = -0; }

    // Double-negation: outer Neg applies to a folded Value, so it is left for downstream.
    fun double_neg_i8() { let _x: i8 = -(-1); }

    // Negation inside an expression should still type-check (the new arm only fires for the
    // direct `Neg(InferredNum)` shape, leaving compound expressions to the normal path).
    fun neg_then_add()  { let _x: i8 = -5 + 3; }
}
