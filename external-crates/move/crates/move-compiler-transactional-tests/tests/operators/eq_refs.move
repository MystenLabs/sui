//# init --edition 2024.alpha

//# publish
module 0x42::m {

    public struct S has copy, drop { t: u64 }

    public fun make_s(t: u64): S {
        S { t }
    }

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

    public fun tnum_0(): bool {
       0 == &0
    }

    public fun tnum_1(): bool {
       &0 == &0
    }

    public fun tnum_2(): bool {
        let a = 0;
        let b = &mut 0;
        let c = &0;
        a == b && b == c && a == c
    }


}

//# run
module 0x43::main {

    fun main() {
        let s_val = 0x42::m::make_s(42);
        let s_ref = &(0x42::m::make_s(42));
        let s_mut = &mut (0x42::m::make_s(42));

        assert!(0x42::m::test_0(s_val, s_val), 0);
        assert!(0x42::m::test_1(s_val, s_ref), 1);
        assert!(0x42::m::test_2(s_val, s_mut), 2);
        assert!(0x42::m::test_3(s_ref, s_val), 3);
        assert!(0x42::m::test_4(s_ref, s_ref), 4);
        assert!(0x42::m::test_5(s_ref, s_mut), 5);
        assert!(0x42::m::test_6(s_mut, s_val), 6);
        assert!(0x42::m::test_7(s_mut, s_ref), 7);
        // assert!(0x42::m::test_8(s_mut, s_mut), 8);
        assert!(0x42::m::test_9(s_val, s_val, s_val), 9);
        assert!(0x42::m::test_10(s_val, s_val, s_ref), 10);
        assert!(0x42::m::test_11(s_val, s_val, s_mut), 11);
        assert!(0x42::m::test_12(s_val, s_ref, s_val), 12);
        assert!(0x42::m::test_13(s_val, s_ref, s_ref), 13);
        assert!(0x42::m::test_14(s_val, s_ref, s_mut), 14);
        assert!(0x42::m::test_15(s_val, s_mut, s_val), 15);
        assert!(0x42::m::test_16(s_val, s_mut, s_ref), 16);
        // assert!(0x42::m::test_17(s_val, s_mut, s_mut), 17);
        assert!(0x42::m::test_18(s_ref, s_val, s_val), 18);
        assert!(0x42::m::test_19(s_ref, s_val, s_ref), 19);
        assert!(0x42::m::test_20(s_ref, s_val, s_mut), 20);
        assert!(0x42::m::test_21(s_ref, s_ref, s_val), 21);
        assert!(0x42::m::test_22(s_ref, s_ref, s_ref), 22);
        assert!(0x42::m::test_23(s_ref, s_ref, s_mut), 23);
        assert!(0x42::m::test_24(s_ref, s_mut, s_val), 24);
        assert!(0x42::m::test_25(s_ref, s_mut, s_ref), 25);
        // assert!(0x42::m::test_26(s_ref, s_mut, s_mut), 26);
        assert!(0x42::m::test_27(s_mut, s_val, s_val), 27);
        assert!(0x42::m::test_28(s_mut, s_val, s_ref), 28);
        // assert!(0x42::m::test_29(s_mut, s_val, s_mut), 29);
        assert!(0x42::m::test_30(s_mut, s_ref, s_val), 30);
        assert!(0x42::m::test_31(s_mut, s_ref, s_ref), 31);
        // assert!(0x42::m::test_32(s_mut, s_ref, s_mut), 32);
        // assert!(0x42::m::test_33(s_mut, s_mut, s_val), 33);
        // assert!(0x42::m::test_34(s_mut, s_mut, s_ref), 34);
        // assert!(0x42::m::test_35(s_mut, s_mut, s_mut), 35);

        let s2_val = 0x42::m::make_s(2);
        let s2_ref = &(0x42::m::make_s(2));
        let s2_mut = &mut (0x42::m::make_s(2));

        let s3_val = 0x42::m::make_s(3);
        let s3_ref = &(0x42::m::make_s(3));
        let s3_mut = &mut (0x42::m::make_s(3));

        assert!(!0x42::m::test_0(s_val, s2_val), 36);
        assert!(!0x42::m::test_1(s_val, s2_ref), 37);
        assert!(!0x42::m::test_2(s_val, s2_mut), 38);
        assert!(!0x42::m::test_3(s_ref, s2_val), 39);
        assert!(!0x42::m::test_4(s_ref, s2_ref), 40);
        assert!(!0x42::m::test_5(s_ref, s2_mut), 41);
        assert!(!0x42::m::test_6(s_mut, s2_val), 42);
        assert!(!0x42::m::test_7(s_mut, s2_ref), 43);
        assert!(!0x42::m::test_8(s_mut, s2_mut), 44);
        assert!(!0x42::m::test_9(s_val, s2_val, s3_val), 45);
        assert!(!0x42::m::test_10(s_val, s2_val, s3_ref), 46);
        assert!(!0x42::m::test_11(s_val, s2_val, s3_mut), 47);
        assert!(!0x42::m::test_12(s_val, s2_ref, s3_val), 48);
        assert!(!0x42::m::test_13(s_val, s2_ref, s3_ref), 49);
        assert!(!0x42::m::test_14(s_val, s2_ref, s3_mut), 50);
        assert!(!0x42::m::test_15(s_val, s2_mut, s3_val), 51);
        assert!(!0x42::m::test_16(s_val, s2_mut, s3_ref), 52);
        assert!(!0x42::m::test_17(s_val, s2_mut, s3_mut), 53);
        assert!(!0x42::m::test_18(s_ref, s2_val, s3_val), 54);
        assert!(!0x42::m::test_19(s_ref, s2_val, s3_ref), 55);
        assert!(!0x42::m::test_20(s_ref, s2_val, s3_mut), 56);
        assert!(!0x42::m::test_21(s_ref, s2_ref, s3_val), 57);
        assert!(!0x42::m::test_22(s_ref, s2_ref, s3_ref), 58);
        assert!(!0x42::m::test_23(s_ref, s2_ref, s3_mut), 59);
        assert!(!0x42::m::test_24(s_ref, s2_mut, s3_val), 60);
        assert!(!0x42::m::test_25(s_ref, s2_mut, s3_ref), 61);
        assert!(!0x42::m::test_26(s_ref, s2_mut, s3_mut), 62);
        assert!(!0x42::m::test_27(s_mut, s2_val, s3_val), 63);
        assert!(!0x42::m::test_28(s_mut, s2_val, s3_ref), 64);
        assert!(!0x42::m::test_29(s_mut, s2_val, s3_mut), 65);
        assert!(!0x42::m::test_30(s_mut, s2_ref, s3_val), 66);
        assert!(!0x42::m::test_31(s_mut, s2_ref, s3_ref), 67);
        assert!(!0x42::m::test_32(s_mut, s2_ref, s3_mut), 68);
        assert!(!0x42::m::test_33(s_mut, s2_mut, s3_val), 69);
        assert!(!0x42::m::test_34(s_mut, s2_mut, s3_ref), 70);
        assert!(!0x42::m::test_35(s_mut, s2_mut, s3_mut), 71);

        assert!(0x42::m::tnum_0(), 101);
        assert!(0x42::m::tnum_1(), 102);
        assert!(0x42::m::tnum_2(), 103);
    }
}
