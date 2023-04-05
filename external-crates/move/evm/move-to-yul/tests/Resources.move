// Use this test to check source info generation:
//
// experiment: capture-source-info
//
#[evm_contract]
module 0x2::M {
    use Evm::Evm::sign;

    struct T has key {
        s: S
    }

    struct S has key, store {
      a: u64,
      b: u8,
      c: S2
    }

    struct S2 has store {
        x: u128
    }

    fun publish(sg: &signer, a: u64) {
        let s = S{a, b: ((a / 2) as u8), c: S2{x: ((a + a) as u128)}};
        move_to<S>(sg, s)
    }
    #[evm_test]
    fun test_publish() acquires S {
       publish(&sign(@3), 22);
       assert!(exists<S>(@3), 100);
       assert!(borrow_global<S>(@3).a == 22, 101);
       assert!(borrow_global<S>(@3).b == 11, 102);
       assert!(borrow_global<S>(@3).c.x == 44, 103);
    }

    fun unpublish(a: address): S acquires S {
        move_from<S>(a)
    }
    #[evm_test]
    fun test_unpublish() acquires S {
       publish(&sign(@3), 33);
       let S{a, b, c: S2{x}} = unpublish(@3);
       assert!(a == 33, 101);
       assert!(b == 16, 102);
       assert!(x == 66, 103);
    }

    fun increment_a(addr: address) acquires S{
        let r = borrow_global_mut<S>(addr);
        r.a = r.a + 1
    }
    #[evm_test]
    fun test_increment_a() acquires S {
        publish(&sign(@3), 510);
        increment_a(@3);
        assert!(borrow_global<S>(@3).a == 511, 100);
    }

    fun publish_t(sg: &signer, a: u64) {
        let t = T { s: S{a, b: ((a / 2) as u8), c: S2{x: ((a + a) as u128)}}};
        move_to<T>(sg, t)
    }
    #[evm_test]
    fun test_publish_t() acquires T {
        publish_t(&sign(@3), 22);
        assert!(exists<T>(@3), 100);
        assert!(borrow_global<T>(@3).s.a == 22, 101);
        assert!(borrow_global<T>(@3).s.b == 11, 102);
        assert!(borrow_global<T>(@3).s.c.x == 44, 103);
    }

}
