module 0x42::a {

    public struct S has drop, copy {}

    public fun test_0(a: S, b: S): bool {
        a == b
    }

    public fun test_1(a: S, b: &S): bool {
        a == b
    }

    public fun test_2(a: S, b: &mut S): bool {
        a == b
    }

    public fun test_3(a: &S, b: S): bool {
        a == b
    }

    public fun test_4(a: &S, b: &S): bool {
        a == b
    }

    public fun test_5(a: &S, b: &mut S): bool {
        a == b
    }

    public fun test_6(a: &mut S, b: S): bool {
        a == b
    }

    public fun test_7(a: &mut S, b: &S): bool {
        a == b
    }

    public fun test_8(a: &mut S, b: &mut S): bool {
        a == b
    }

    public fun test_9(a: S, b: S, c: S): bool {
        a == b && b == c && a == c
    }

    public fun test_10(a: S, b: S, c: &S): bool {
        a == b && b == c && a == c
    }

    public fun test_11(a: S, b: S, c: &mut S): bool {
        a == b && b == c && a == c
    }

    public fun test_12(a: S, b: &S, c: S): bool {
        a == b && b == c && a == c
    }

    public fun test_13(a: S, b: &S, c: &S): bool {
        a == b && b == c && a == c
    }

    public fun test_14(a: S, b: &S, c: &mut S): bool {
        a == b && b == c && a == c
    }

    public fun test_15(a: S, b: &mut S, c: S): bool {
        a == b && b == c && a == c
    }

    public fun test_16(a: S, b: &mut S, c: &S): bool {
        a == b && b == c && a == c
    }

    public fun test_17(a: S, b: &mut S, c: &mut S): bool {
        a == b && b == c && a == c
    }

    public fun test_18(a: &S, b: S, c: S): bool {
        a == b && b == c && a == c
    }

    public fun test_19(a: &S, b: S, c: &S): bool {
        a == b && b == c && a == c
    }

    public fun test_20(a: &S, b: S, c: &mut S): bool {
        a == b && b == c && a == c
    }

    public fun test_21(a: &S, b: &S, c: S): bool {
        a == b && b == c && a == c
    }

    public fun test_22(a: &S, b: &S, c: &S): bool {
        a == b && b == c && a == c
    }

    public fun test_23(a: &S, b: &S, c: &mut S): bool {
        a == b && b == c && a == c
    }

    public fun test_24(a: &S, b: &mut S, c: S): bool {
        a == b && b == c && a == c
    }

    public fun test_25(a: &S, b: &mut S, c: &S): bool {
        a == b && b == c && a == c
    }

    public fun test_26(a: &S, b: &mut S, c: &mut S): bool {
        a == b && b == c && a == c
    }

    public fun test_27(a: &mut S, b: S, c: S): bool {
        a == b && b == c && a == c
    }

    public fun test_28(a: &mut S, b: S, c: &S): bool {
        a == b && b == c && a == c
    }

    public fun test_29(a: &mut S, b: S, c: &mut S): bool {
        a == b && b == c && a == c
    }

    public fun test_30(a: &mut S, b: &S, c: S): bool {
        a == b && b == c && a == c
    }

    public fun test_31(a: &mut S, b: &S, c: &S): bool {
        a == b && b == c && a == c
    }

    public fun test_32(a: &mut S, b: &S, c: &mut S): bool {
        a == b && b == c && a == c
    }

    public fun test_33(a: &mut S, b: &mut S, c: S): bool {
        a == b && b == c && a == c
    }

    public fun test_34(a: &mut S, b: &mut S, c: &S): bool {
        a == b && b == c && a == c
    }

    public fun test_35(a: &mut S, b: &mut S, c: &mut S): bool {
        a == b && b == c && a == c
    }

}
