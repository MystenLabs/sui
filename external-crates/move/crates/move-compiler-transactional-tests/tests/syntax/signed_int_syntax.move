//# init --edition development

//# publish
module 0x42::signed_syntax {

    // Struct with signed integer fields
    public struct Point has copy, drop {
        x: i32,
        y: i32,
    }

    // Struct with multiple signed types
    public struct MultiSigned has copy, drop {
        a: i8,
        b: i16,
        c: i32,
        d: i64,
        e: i128,
    }

    public fun test_let_bindings() {
        // Signed ints in let bindings
        let x: i32 = -42i32;
        let y: i8 = 0i8;
        let z: i64 = 9223372036854775807i64;
        let w: i16 = -32768i16;

        assert!(x == -42i32, 0);
        assert!(y == 0i8, 1);
        assert!(z == 9223372036854775807i64, 2);
        assert!(w == -32768i16, 3);
    }

    public fun test_struct_fields() {
        // Signed ints in struct construction
        let p = Point { x: -10i32, y: 20i32 };
        assert!(p.x == -10i32, 0);
        assert!(p.y == 20i32, 1);

        let m = MultiSigned {
            a: -1i8,
            b: -2i16,
            c: -3i32,
            d: -4i64,
            e: -5i128,
        };
        assert!(m.a == -1i8, 2);
        assert!(m.b == -2i16, 3);
        assert!(m.c == -3i32, 4);
        assert!(m.d == -4i64, 5);
        assert!(m.e == -5i128, 6);
    }

    public fun test_tuple_positions() {
        // Signed ints in tuple positions
        let (a, b, c): (i8, i32, i64) = (-1i8, -2i32, -3i64);
        assert!(a == -1i8, 0);
        assert!(b == -2i32, 1);
        assert!(c == -3i64, 2);
    }

    public fun test_vector_literals() {
        // Signed ints in vector via push (vector literal inference for
        // signed types is tested separately in signed_int_vectors).
        let mut v: vector<i8> = vector[];
        v.push_back(1i8);
        v.push_back(-1i8);
        v.push_back(0i8);
        assert!(v.length() == 3, 0);
        assert!(*v.borrow(0) == 1i8, 1);
        assert!(*v.borrow(1) == -1i8, 2);
        assert!(*v.borrow(2) == 0i8, 3);

        let mut w: vector<i32> = vector[];
        w.push_back(-100i32);
        w.push_back(0i32);
        w.push_back(100i32);
        assert!(w.length() == 3, 4);
    }

    public fun test_type_annotations() {
        // Explicit type annotations with signed ints
        let _x: i8 = 42i8;
        let _y: i16 = -1000i16;
        let _z: i32 = 0i32;
        let _w: i64 = -1i64;
        let _v: i128 = 1i128;
        let _u: i256 = -1i256;
    }

    public fun test_if_expressions() {
        // Signed ints in if expressions
        let x: i32 = if (true) -1i32 else 1i32;
        assert!(x == -1i32, 0);

        let y: i64 = if (false) 100i64 else -100i64;
        assert!(y == -100i64, 1);
    }
}

//# run 0x42::signed_syntax::test_let_bindings

//# run 0x42::signed_syntax::test_struct_fields

//# run 0x42::signed_syntax::test_tuple_positions

//# run 0x42::signed_syntax::test_vector_literals

//# run 0x42::signed_syntax::test_type_annotations

//# run 0x42::signed_syntax::test_if_expressions
