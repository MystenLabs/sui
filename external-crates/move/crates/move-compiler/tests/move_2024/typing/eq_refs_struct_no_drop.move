module 0x42::a {

    public struct S has copy {}

    public fun test_4(a: &S, b: &S): bool {
        a == b
    }

    public fun test_5(a: &S, b: &mut S): bool {
        a == b
    }

    public fun test_7(a: &mut S, b: &S): bool {
        a == b
    }

    public fun test_8(a: &mut S, b: &mut S): bool {
        a == b
    }

    public fun test_22(a: &S, b: &S, c: &S): bool {
        a == b && b == c && a == c
    }

    public fun test_23(a: &S, b: &S, c: &mut S): bool {
        a == b && b == c && a == c
    }

    public fun test_25(a: &S, b: &mut S, c: &S): bool {
        a == b && b == c && a == c
    }

    public fun test_26(a: &S, b: &mut S, c: &mut S): bool {
        a == b && b == c && a == c
    }

    public fun test_31(a: &mut S, b: &S, c: &S): bool {
        a == b && b == c && a == c
    }

    public fun test_32(a: &mut S, b: &S, c: &mut S): bool {
        a == b && b == c && a == c
    }

    public fun test_34(a: &mut S, b: &mut S, c: &S): bool {
        a == b && b == c && a == c
    }

    public fun test_35(a: &mut S, b: &mut S, c: &mut S): bool {
        a == b && b == c && a == c
    }

}
