// tests signed integers as struct fields
module a::m {
    public struct S has drop {
        x: i64,
        y: i8,
    }

    fun create(): S {
        S { x: 42i64, y: 1i8 }
    }
}
