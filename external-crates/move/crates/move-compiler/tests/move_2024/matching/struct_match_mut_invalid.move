//# init

//# publish
module 0x42::m {

    public struct NBase has copy, drop { t: u64 }

    public struct PBase(u64) has copy, drop;

    public struct NPoly<T> has copy, drop { t: T }

    public struct PPoly<T>(T) has copy, drop;

    public fun make_nbase(): NBase {
        NBase { t: 0 }
    }

    public fun make_pbase(): PBase {
        PBase(0)
    }

    public fun make_npoly<T>(t: T): NPoly<T> {
        NPoly { t }
    }

    public fun make_ppoly<T>(t: T): PPoly<T> {
        PPoly(t)
    }

    public fun test_00(s: &NBase): u64 {
        match (s) {
           NBase { mut t } => {
                t = t + 1;
                0
           },
        }
    }

    public fun test_01(s: &NBase): u64 {
        match (s) {
           NBase { t: mut x } => {
                x = x + 1;
                0
           },
        }
    }

    public fun test_02(s: &PBase): u64 {
        match (s) {
           PBase(mut x) => {
               x = x + 1;
               0
           }
        }
    }

    public fun test_03(s: &NPoly<NBase>): u64 {
        match (s) {
           NPoly { t : NBase { mut t } } => {
                t = t + 1;
                0
           },
        }
    }

    public fun test_04(s: &NPoly<NBase>): u64 {
        match (s) {
           NPoly { t : NBase { t: mut x } } => {
                x = x + 1;
                0
           },
        }
    }

    public fun test_05(s: &NPoly<PBase>): u64 {
        match (s) {
           NPoly { t : PBase(mut x) } => {
                x = x + 1;
                0
           },
        }
    }

    public fun test_06(s: &PPoly<NBase>): u64 {
        match (s) {
           PPoly(NBase { t: mut x }) => {
                x = x + 1;
                0
           },
        }
    }

    public fun test_07(s: &PPoly<PBase>): u64 {
        match (s) {
           PPoly(PBase(mut x)) => {
                x = x + 1;
                0
           },
        }
    }

}

//# run
module 0x42::main {

    fun main() {
        use 0x42::m::{make_nbase, make_pbase, make_npoly, make_ppoly};

        assert!(0x42::m::test_00(&make_nbase()) == 1, 0);
        assert!(0x42::m::test_01(&make_nbase()) == 1, 1);
        assert!(0x42::m::test_02(&make_pbase()) == 1, 2);
        assert!(0x42::m::test_03(&make_npoly(make_nbase())) == 1, 3);
        assert!(0x42::m::test_04(&make_npoly(make_nbase())) == 1, 4);
        assert!(0x42::m::test_05(&make_npoly(make_pbase())) == 1, 5);
        assert!(0x42::m::test_06(&make_ppoly(make_nbase())) == 1, 6);
        assert!(0x42::m::test_07(&make_ppoly(make_pbase())) == 1, 7);
    }
}
