// Tests basic arithmetic. We only test for u64. Existing move unit tests (once ready for Move on EVM) should cover
// all other basic types.
#[evm_contract]
module 0x2::M {

    // ==============================

    #[callable]
    fun add_two_number(x: u64, y: u64): (u64, u64) {
        let res: u64 = x + y;
        let z: u64 = 3;
        (z, res)
    }
    #[evm_test]
    fun test_add_two_number() {
        let (z, res) = add_two_number(2, 5);
        assert!(z == 3, 100);
        assert!(res == 7, 101);
    }
    #[evm_test]
    fun test_add_two_number_wrong_assert() {
        let (z, res) = add_two_number(2, 5);
        assert!(z == 3, 100);
        assert!(res == 6, 101);
    }
    #[evm_test]
    fun test_add_two_number_overflow() {
        let (_z, _res) = add_two_number(18446744073709551615, 1);
    }

    // ==============================

    #[callable]
    fun div(x: u64, y: u64): (u64, u64) {
        (x / y, x % y)
    }
    #[evm_test]
    fun test_div() {
        let (r1, r2) = div(7, 4);
        assert!(r1 == 1, 100);
        assert!(r2 == 3, 101);
    }
    #[evm_test]
    fun test_div_wrong_assert() {
        let (r1, r2) = div(7, 4);
        assert!(r1 == 1, 100);
        assert!(r2 == 2, 101);
    }
    #[evm_test]
    fun test_div_by_zero() {
        let (_r1, _r2) = div(7, 0);
    }


    // ==============================

    #[callable]
    fun multiple_ops(x: u64, y: u64, z: u64): u64 {
        x + y * z
    }
    #[evm_test]
    fun test_multiple_ops() {
        let r = multiple_ops(3, 2, 5);
        assert!(r == 3 + 2 * 5, 100);
    }
    #[evm_test]
    fun test_multiple_overflow() {
        let r = multiple_ops(0, 18446744073709551615, 2);
        assert!(r == 0, 100);
    }

    // ==============================

    #[callable]
    fun bool_ops(a: u64, b: u64): (bool, bool) {
        let c: bool;
        let d: bool;
        c = a > b && a >= b;
        d = a < b || a <= b;
        if (!(c != d)) abort 42;
        (c, d)
    }
    #[evm_test]
    fun test_bool_ops() {
        let (r1, r2) = bool_ops(3, 2);
        assert!(r1 == true, 100);
        assert!(r2 == false, 101);
    }

    #[evm_test]
    fun test_bool_ops_aborts() {
        let (r1, r2) = (true, false);
        assert!(!(r1 == r2), 100);
        assert!(r1 != r2, 101);
        assert!(!r1 != !r2, 102);
        assert!(!r2, 103);
        assert!(!r1, 104); // should abort here
    }

    // ==============================

    #[callable]
    fun arithmetic_ops(a: u64): (u64, u64) {
        let c: u64;
        c = (6 + 4 - 1) * 2 / 3 % 4;
        if (c != 2) abort 42;
        (c, a)
    }
    #[evm_test]
    fun test_arithmetic_ops_aborts() {
        let (r1, r2) = arithmetic_ops(3);
        assert!(r1 == 1, 100); // should abort here
        assert!(r2 == 3, 101);
    }

    // ==============================

    #[callable]
    fun underflow(): u64 {
        let x = 0;
        x - 1
    }
    #[evm_test]
    fun test_underflow() {
        let _r = underflow();
    }
}
