#[evm_contract]
module 0x2::M {
    use std::vector;
    use Evm::Evm::{sign, require};

    struct T has key, drop {
        s: vector<u8>
    }

    #[evm_test]
    fun h1() acquires T {
        let x = b"abc";
        let t = T { s: x } ;
        move_to<T>(&sign(@3), t);
        let v = borrow_global<T>(@3).s;
        assert!(vector::length(&v) == 3, 96);
        assert!(*vector::borrow(&v, 0) == 97, 97);
        assert!(*vector::borrow(&v, 1) == 98u8, 98);
        assert!(*vector::borrow(&v, 2) == 99u8, 99);
        borrow_global_mut<T>(@3).s = b"efgh";
        let v = borrow_global<T>(@3).s;
        assert!(vector::length(&v) == 4, 100);
        assert!(*vector::borrow(&v, 0) == 101u8, 101);
        assert!(*vector::borrow(&v, 1) == 102u8, 102);
        assert!(*vector::borrow(&v, 2) == 103u8, 103);
        assert!(*vector::borrow(&v, 3) == 104u8, 104);

    }

    #[evm_test]
    public fun test_same_literals() {
        require(true, b"error_message");
        require(true, b"error_message");
    }

}
