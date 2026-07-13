// Tests that signed integer types are rejected in struct fields, function
// parameters, return types, and constants in 2024 edition.
module a::m {
    public struct S has drop {
        x: i32,
        y: i64,
        z: i256,
    }

    fun params(_a: i8, _b: i128): i64 {
        0
    }

    const MY_CONST: i32 = 0;
}
