//# init --edition 2024.beta

//# publish
module 0x42::m {

    public struct NBase has copy, drop { t: u64 }

    public struct PBase(u64) has copy, drop;

    public struct PEmpty() has copy, drop;

    public struct NEmpty{} has copy, drop;

    public struct NPoly<T> has copy, drop { t: T }

    public struct PPoly<T>(T) has copy, drop;

    public fun make_nbase(): NBase {
        NBase { t: 0 }
    }

    public fun make_pbase(): PBase {
        PBase(0)
    }

    public fun make_nempty(): NEmpty {
        NEmpty { }
    }

    public fun make_pempty(): PEmpty {
        PEmpty()
    }

    public fun make_npoly<T>(t: T): NPoly<T> {
        NPoly { t }
    }

    public fun make_ppoly<T>(t: T): PPoly<T> {
        PPoly(t)
    }

    public fun test_00(s: NBase): u64 {
        match (s) {
           NBase { t: 0 } => 0,
           NBase { t: x } => x,
        }
    }

    public fun test_01(s: PBase): u64 {
        match (s) {
           PBase(0) => 1,
           PBase(x) => x,
        }
    }

    public fun test_02(s: &NBase): u64 {
        match (s) {
           NBase { t: 0 } => 2,
           NBase { t: x } => *x,
        }
    }

    public fun test_03(s: &PBase): u64 {
        match (s) {
           PBase(0) => 3,
           PBase(x) => *x,
        }
    }

    public fun test_04(s: &mut NBase): u64 {
        match (s) {
           NBase { t: 0 } => 4,
           NBase { t: x } => *x,
        }
    }

    public fun test_05(s: &mut PBase): u64 {
        match (s) {
           PBase(0) => 5,
           PBase(x) => *x,
        }
    }

    public fun test_06(s: NPoly<NBase>): u64 {
        match (s) {
           NPoly { t: NBase { t: 0 } } => 6,
           NPoly { t: NBase { t: x } } => x,
        }
    }

    public fun test_07(s: NPoly<PBase>): u64 {
        match (s) {
           NPoly { t: PBase(0) } => 7,
           NPoly { t: PBase(x) } => x,
        }
    }

    public fun test_08(s: PPoly<NBase>): u64 {
        match (s) {
           PPoly(NBase { t: 0 }) => 8,
           PPoly(NBase { t: x }) => x,
        }
    }

    public fun test_09(s: PPoly<PBase>): u64 {
        match (s) {
           PPoly(PBase(0)) => 9,
           PPoly(PBase(x)) => x,
        }
    }

    public fun test_10(s: &NPoly<NBase>): u64 {
        match (s) {
           NPoly { t: NBase { t: 0 } } => 10,
           NPoly { t: NBase { t: x } } => *x,
        }
    }

    public fun test_11(s: &NPoly<PBase>): u64 {
        match (s) {
           NPoly { t: PBase(0) } => 11,
           NPoly { t: PBase(x) } => *x,
        }
    }

    public fun test_12(s: &PPoly<NBase>): u64 {
        match (s) {
           PPoly(NBase { t: 0 }) => 12,
           PPoly(NBase { t: x }) => *x,
        }
    }

    public fun test_13(s: &PPoly<PBase>): u64 {
        match (s) {
           PPoly(PBase(0)) => 13,
           PPoly(PBase(x)) => *x,
        }
    }

    public fun test_14(s: &mut NPoly<NBase>): u64 {
        match (s) {
           NPoly { t: NBase { t: 0 } } => 14,
           NPoly { t: NBase { t: x } } => *x,
        }
    }

    public fun test_15(s: &mut NPoly<PBase>): u64 {
        match (s) {
           NPoly { t: PBase(0) } => 15,
           NPoly { t: PBase(x) } => *x,
        }
    }

    public fun test_16(s: &mut PPoly<NBase>): u64 {
        match (s) {
           PPoly(NBase { t: 0 }) => 16,
           PPoly(NBase { t: x }) => *x,
        }
    }

    public fun test_17(s: &mut PPoly<PBase>): u64 {
        match (s) {
           PPoly(PBase(0)) => 17,
           PPoly(PBase(x)) => *x,
        }
    }

    public fun test_18(s: &mut NPoly<NEmpty>): u64 {
        match (s) {
           NPoly { t: NEmpty { } } => 18,
        }
    }

    public fun test_19(s: &mut NPoly<NEmpty>): u64 {
        match (s) {
           NPoly { t: NEmpty { } } => 19,
        }
    }

    public fun test_20(s: &mut PPoly<NEmpty>): u64 {
        match (s) {
           PPoly(NEmpty { }) => 20,
        }
    }

    public fun test_21(s: &mut PPoly<NEmpty>): u64 {
        match (s) {
           PPoly(NEmpty { }) => 21,
        }
    }

    public fun test_22(s: &mut NPoly<PEmpty>): u64 {
        match (s) {
           NPoly { t: PEmpty() } => 22,
        }
    }

    public fun test_23(s: &mut NPoly<PEmpty>): u64 {
        match (s) {
           NPoly { t: PEmpty() } => 23,
        }
    }

    public fun test_24(s: &mut PPoly<PEmpty>): u64 {
        match (s) {
           PPoly(PEmpty()) => 24,
        }
    }

    public fun test_25(s: &mut PPoly<PEmpty>): u64 {
        match (s) {
           PPoly(PEmpty()) => 25,
        }
    }

    public fun test_26(s: NPoly<NBase>): u64 {
        match (s) {
           NPoly { t: NBase { t: 0 } } => 26,
           NPoly { t: _ } => 0,
        }
    }

    public fun test_27(s: NPoly<PBase>): u64 {
        match (s) {
           NPoly { t: PBase(0) } => 27,
           NPoly { t: _ } => 0,
        }
    }

    public fun test_28(s: PPoly<NBase>): u64 {
        match (s) {
           PPoly(NBase { t: 0 }) => 28,
           PPoly(_) => 0,
        }
    }

    public fun test_29(s: PPoly<PBase>): u64 {
        match (s) {
           PPoly(PBase(0)) => 29,
           PPoly(_) => 29,
        }
    }

    public fun test_30(s: &NPoly<NBase>): u64 {
        match (s) {
           NPoly { t: NBase { t: _ } } => 30,
        }
    }

    public fun test_31(s: &NPoly<PBase>): u64 {
        match (s) {
           NPoly { t: PBase(_) } => 31,
        }
    }

    public fun test_32(s: &PPoly<NBase>): u64 {
        match (s) {
           PPoly(NBase { t: _ }) => 32,
        }
    }

    public fun test_33(s: &PPoly<PBase>): u64 {
        match (s) {
           PPoly(PBase(_)) => 33,
        }
    }

    public fun test_34(s: &mut NPoly<NBase>): u64 {
        match (s) {
           NPoly { t: NBase { t: _ } } => 34,
        }
    }

    public fun test_35(s: &mut NPoly<PBase>): u64 {
        match (s) {
           NPoly { t: PBase(_) } => 35,
        }
    }

    public fun test_36(s: &mut PPoly<NBase>): u64 {
        match (s) {
           PPoly(NBase { t: _ }) => 36,
        }
    }

    public fun test_37(s: &mut PPoly<PBase>): u64 {
        match (s) {
           PPoly(PBase(_)) => 37,
        }
    }
}

//# run
module 0x43::main {

    fun main() {
        use 0x42::m::{make_nbase, make_pbase, make_npoly, make_ppoly, make_nempty, make_pempty};

        assert!(0x42::m::test_00(make_nbase()) == 0, 0);
        assert!(0x42::m::test_01(make_pbase()) == 1, 1);
        assert!(0x42::m::test_02(&make_nbase()) == 2, 2);
        assert!(0x42::m::test_03(&make_pbase()) == 3, 3);
        assert!(0x42::m::test_04(&mut make_nbase()) == 4, 4);
        assert!(0x42::m::test_05(&mut make_pbase()) == 5, 5);
        assert!(0x42::m::test_06(make_npoly(make_nbase())) == 6, 6);
        assert!(0x42::m::test_07(make_npoly(make_pbase())) == 7, 7);
        assert!(0x42::m::test_08(make_ppoly(make_nbase())) == 8, 8);
        assert!(0x42::m::test_09(make_ppoly(make_pbase())) == 9, 9);
        assert!(0x42::m::test_10(&make_npoly(make_nbase())) == 10, 10);
        assert!(0x42::m::test_11(&make_npoly(make_pbase())) == 11, 11);
        assert!(0x42::m::test_12(&make_ppoly(make_nbase())) == 12, 12);
        assert!(0x42::m::test_13(&make_ppoly(make_pbase())) == 13, 13);
        assert!(0x42::m::test_14(&mut make_npoly(make_nbase())) == 14, 14);
        assert!(0x42::m::test_15(&mut make_npoly(make_pbase())) == 15, 15);
        assert!(0x42::m::test_16(&mut make_ppoly(make_nbase())) == 16, 16);
        assert!(0x42::m::test_17(&mut make_ppoly(make_pbase())) == 17, 17);
        assert!(0x42::m::test_18(&mut make_npoly(make_nempty())) == 18, 18);
        assert!(0x42::m::test_19(&mut make_npoly(make_nempty())) == 19, 19);
        assert!(0x42::m::test_20(&mut make_ppoly(make_nempty())) == 20, 20);
        assert!(0x42::m::test_21(&mut make_ppoly(make_nempty())) == 21, 21);
        assert!(0x42::m::test_22(&mut make_npoly(make_pempty())) == 22, 22);
        assert!(0x42::m::test_23(&mut make_npoly(make_pempty())) == 23, 23);
        assert!(0x42::m::test_24(&mut make_ppoly(make_pempty())) == 24, 24);
        assert!(0x42::m::test_25(&mut make_ppoly(make_pempty())) == 25, 25);
        assert!(0x42::m::test_26(make_npoly(make_nbase())) == 26, 26);
        assert!(0x42::m::test_27(make_npoly(make_pbase())) == 27, 27);
        assert!(0x42::m::test_28(make_ppoly(make_nbase())) == 28, 28);
        assert!(0x42::m::test_29(make_ppoly(make_pbase())) == 29, 29);
        assert!(0x42::m::test_30(&make_npoly(make_nbase())) == 30, 30);
        assert!(0x42::m::test_31(&make_npoly(make_pbase())) == 31, 31);
        assert!(0x42::m::test_32(&make_ppoly(make_nbase())) == 32, 32);
        assert!(0x42::m::test_33(&make_ppoly(make_pbase())) == 33, 33);
        assert!(0x42::m::test_34(&mut make_npoly(make_nbase())) == 34, 34);
        assert!(0x42::m::test_35(&mut make_npoly(make_pbase())) == 35, 35);
        assert!(0x42::m::test_36(&mut make_ppoly(make_nbase())) == 36, 36);
        assert!(0x42::m::test_37(&mut make_ppoly(make_pbase())) == 37, 37);
    }
}
