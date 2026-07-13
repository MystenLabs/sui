// Tests that signed integer constants are rejected when the feature is not enabled.
module a::m {
    const C_I8: i8 = 1i8;
    const C_I16: i16 = 1i16;
    const C_I32: i32 = 1i32;
    const C_I64: i64 = 1i64;
    const C_I128: i128 = 1i128;
    const C_I256: i256 = 1i256;
}
