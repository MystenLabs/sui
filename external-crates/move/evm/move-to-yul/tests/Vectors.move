#[evm_contract]
module 0x2::Vectors {
    use std::vector;

    struct S has copy, drop { x: u128, y: bool, z: u64 }
    struct R has copy, drop { s: S, v: vector<u64> }

    fun empty_vector() : vector<u64> {
        vector::empty<u64>()
    }

    #[evm_test]
    fun test_empty() {
        let v = empty_vector();
        assert!(vector::length(&v) == 0, 101);
    }

    #[evm_test]
    fun test_borrow_fail() {
        let v = empty_vector();
        assert!(*vector::borrow(&v, 0) == 0, 101);
    }

    fun one_elem_u64() : vector<u64>{
        let v = vector::empty<u64>();
        vector::push_back(&mut v, 42);
        v
    }

    #[evm_test]
    #[callable]
    fun test_one_elem_u64() {
        let v = one_elem_u64();
        assert!(vector::length(&v) == 1, 101);
        assert!(*vector::borrow(&v, 0) == 42, 102);
    }


    fun one_elem_struct() : vector<S>{
        let v = vector::empty<S>();
        vector::push_back(&mut v, S { x: 42, y: true, z: 789 });
        v
    }

    #[evm_test]
    fun test_one_elem_struct() {
        let v = one_elem_struct();
        assert!(vector::length(&v) == 1, 101);
        assert!(*&vector::borrow(&v, 0).x == 42, 102);
        assert!(*&vector::borrow(&v, 0).y == true, 103);
        assert!(*&vector::borrow(&v, 0).z == 789, 104);
    }

    #[evm_test]
    fun test_push_back() {
        let v = one_elem_u64();
        assert!(vector::length(&v) == 1, 101);
        assert!(*vector::borrow(&v, 0) == 42, 102);

        vector::push_back(&mut v, 43);
        assert!(vector::length(&v) == 2, 103);
        assert!(*vector::borrow(&v, 0) == 42, 104);
        assert!(*vector::borrow(&v, 1) == 43, 105);

        vector::push_back(&mut v, 44);
        assert!(vector::length(&v) == 3, 106);
        assert!(*vector::borrow(&v, 0) == 42, 107);
        assert!(*vector::borrow(&v, 1) == 43, 108);
        assert!(*vector::borrow(&v, 2) == 44, 109);

        vector::push_back(&mut v, 45);
        assert!(vector::length(&v) == 4, 110);
        assert!(*vector::borrow(&v, 0) == 42, 111);
        assert!(*vector::borrow(&v, 1) == 43, 112);
        assert!(*vector::borrow(&v, 2) == 44, 113);
        assert!(*vector::borrow(&v, 3) == 45, 114);
    }

    #[evm_test]
    fun test_swap() {
        let v = one_elem_u64();
        vector::push_back(&mut v, 43);
        vector::push_back(&mut v, 44);
        assert!(*vector::borrow(&v, 0) == 42, 101);
        assert!(*vector::borrow(&v, 1) == 43, 102);
        assert!(*vector::borrow(&v, 2) == 44, 103);

        vector::swap(&mut v, 0, 2);
        assert!(*vector::borrow(&v, 0) == 44, 104);
        assert!(*vector::borrow(&v, 1) == 43, 105);
        assert!(*vector::borrow(&v, 2) == 42, 106);
    }

    #[evm_test]
    fun test_swap_fail() {
        let v = one_elem_u64();
        vector::push_back(&mut v, 34);
        vector::swap(&mut v, 1, 2);
    }

    #[evm_test]
    fun test_pop_back() {
        let v = one_elem_u64();
        vector::push_back(&mut v, 43);
        assert!(vector::length(&v) == 2, 101);
        assert!(*vector::borrow(&v, 0) == 42, 102);
        assert!(*vector::borrow(&v, 1) == 43, 103);

        let e = vector::pop_back(&mut v);
        assert!(vector::length(&v) == 1, 104);
        assert!(e == 43, 105);

        let e = vector::pop_back(&mut v);
        assert!(vector::length(&v) == 0, 106);
        assert!(e == 42, 107);

        vector::destroy_empty(v);
    }

    #[evm_test]
    fun test_pop_back_empty_fail() {
        let v = vector::empty<address>();
        vector::pop_back(&mut v); // should abort here
    }

    #[evm_test]
    fun test_destroy_empty() {
        let v = vector::empty<address>();
        vector::destroy_empty(v);
    }

    #[evm_test]
    fun test_destroy_non_empty_fail() {
        let v = one_elem_struct();
        vector::destroy_empty(v); // should abort here
    }

    #[evm_test]
    fun test_borrow_mut() {
        let v = one_elem_struct();
        vector::push_back(&mut v, S { x: 45, y: false, z: 123 });
        let s1_ref = vector::borrow_mut(&mut v, 0);
        *&mut s1_ref.x = 90;
        *&mut s1_ref.y = false;
        *&mut s1_ref.z = 1028;
        // the first element is properly changed
        assert!(*&vector::borrow(&v, 0).x == 90, 101);
        assert!(*&vector::borrow(&v, 0).y == false, 102);
        assert!(*&vector::borrow(&v, 0).z == 1028, 103);
        // the second element is not changed
        assert!(*&vector::borrow(&v, 1).x == 45, 104);
        assert!(*&vector::borrow(&v, 1).y == false, 105);
        assert!(*&vector::borrow(&v, 1).z == 123, 106);
        // TODO: uncomment this after we've implemented equality for struct
        // assert!(*vector::borrow(&v, 0) == S { x: 90, y: false, z: 1028 }, 104);

        let s2_ref = vector::borrow_mut(&mut v, 1);
        *&mut s2_ref.x = 10;
        *&mut s2_ref.y = true;
        *&mut s2_ref.z = 456;
        assert!(*&vector::borrow(&v, 1).x == 10, 107);
        assert!(*&vector::borrow(&v, 1).y == true, 108);
        assert!(*&vector::borrow(&v, 1).z == 456, 109);
    }

    #[evm_test]
    fun test_nested_vectors() {
        // construct three vectors
        let v0 = vector::empty<u64>();
        vector::push_back(&mut v0, 10);
        vector::push_back(&mut v0, 11);

        let v1 = vector::empty<u64>();
        vector::push_back(&mut v1, 12);
        vector::push_back(&mut v1, 13);
        vector::push_back(&mut v1, 14);

        let v2 = vector::empty<u64>();
        vector::push_back(&mut v2, 15);
        vector::push_back(&mut v2, 16);
        vector::push_back(&mut v2, 17);
        vector::push_back(&mut v2, 18);

        // push all three vectors into another vector
        let v = vector::empty<vector<u64>>();
        vector::push_back(&mut v, v0);
        vector::push_back(&mut v, v1);
        vector::push_back(&mut v, v2);


        assert!(vector::length(&v) == 3, 101);
        assert!(vector::length(vector::borrow(&v, 0)) == 2, 102);
        assert!(vector::length(vector::borrow(&v, 1)) == 3, 103);
        assert!(vector::length(vector::borrow(&v, 2)) == 4, 104);

        assert!(*vector::borrow(vector::borrow(&v, 0), 0) == 10, 105);
        assert!(*vector::borrow(vector::borrow(&v, 1), 1) == 13, 105);
        assert!(*vector::borrow(vector::borrow(&v, 2), 2) == 17, 105);
        assert!(*vector::borrow(vector::borrow(&v, 2), 3) == 18, 105);
    }

    #[evm_test]
    fun test_vectors_in_structs() {
        let v = vector::empty<u64>();
        vector::push_back(&mut v, 10);
        vector::push_back(&mut v, 11);
        vector::push_back(&mut v, 12);

        let r = R{ s: S{ x: 42, y: true, z: 9 }, v };
        assert!(vector::length(&r.v) == 3, 101);
        assert!(*vector::borrow(&r.v, 0) == 10, 102);
        assert!(*vector::borrow(&r.v, 1) == 11, 103);
        assert!(*vector::borrow(&r.v, 2) == 12, 104);

        *vector::borrow_mut(&mut r.v, 1) = 41;
        assert!(*vector::borrow(&r.v, 1) == 41, 105);

        *&mut r.v = one_elem_u64();
        assert!(vector::length(&r.v) == 1, 106);
        assert!(*vector::borrow(&r.v, 0) == 42, 107);
    }

    #[evm_test]
    fun test_vector_equality() {
        let v = vector::empty<u8>();
        vector::push_back(&mut v, 10);
        vector::push_back(&mut v, 11);
        vector::push_back(&mut v, 12);
        vector::push_back(&mut v, 13);

        assert!(v == x"0a0b0c0d", 101);
        assert!(!(v != x"0a0b0c0d"), 102);
        assert!(v != x"0a0b0c", 103);
        assert!(!(v == x"0a0b0c"), 104);

        vector::push_back(&mut v, 14);
        assert!(v == x"0a0b0c0d0e", 105);
    }

    #[evm_test]
    fun test_vector_equality_struct() {
        let v1 = vector::empty<R>();
        let v2 = vector::empty<R>();
        assert!(v1 == v2, 101);
        let r1 = R{ s: S{ x: 42, y: true, z: 9 }, v: one_elem_u64() };
        vector::push_back(&mut v1, copy r1);
        assert!(v1 != v2, 102);
        vector::push_back(&mut v2, r1);
        assert!(v1 == v2, 103);

        let r2 = R{ s: S{ x: 42, y: false, z: 9 }, v: one_elem_u64() };
        vector::push_back(&mut v1, copy r1);
        assert!(v1 != v2, 104);
        vector::push_back(&mut v2, r2);
        assert!(v1 != v2, 105);
    }

    #[evm_test]
    fun test_nested_vector_equality() {
        let v1 = vector::empty<vector<u8>>();
        let v2 = vector::empty<vector<u8>>();
        assert!(v1 == v2, 101);
        vector::push_back(&mut v1, b"abc");
        vector::push_back(&mut v2, b"abc");
        assert!(v1 == v2, 102);
        vector::push_back(&mut v1, b"def");
        vector::push_back(&mut v2, b"def");
        assert!(v1 == v2, 103);
        vector::push_back(&mut v1, b"ghi");
        vector::push_back(&mut v2, b"ghij");
        assert!(v1 != v2, 104);
    }
}
