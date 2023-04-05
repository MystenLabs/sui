#[evm_contract]
module 0x2::M {
    #[callable]
    fun evaded(a: u64, b: u64): (u64, u64, u64, u64) {
        let c = a;
        let d = c + b;
        let ar = &mut a;
        let cr = &c;
        *ar = *cr + 1;
        (a, b, c, d)
    }

    #[evm_test]
    fun test_evaded() {
        let (a, b, c, d) = evaded(1, 2);
        assert!(a == 2, 100);
        assert!(b == 2, 101);
        assert!(c == 1, 102);
        assert!(d == 3, 103);
    }

    fun call_by_ref(a: &mut u64) {
        *a = 2;
    }

    #[evm_test]
    fun test_call_by_ref() {
        let a = 1;
        //assert!(a == 1, 100);
        call_by_ref(&mut a);
        assert!(a == 2, 101);
    }

    fun call_by_immut_ref(a: &u64) : u64 {
        *a
    }

    #[evm_test]
    fun test_freeze_ref() {
        let a = 1;
        a = call_by_immut_ref(&mut a);
        assert!(a == 1, 101);
    }
}
