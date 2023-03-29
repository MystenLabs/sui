#[test_only]
module extensions::table_tests {
    use std::vector;
    use extensions::table as T;

    struct S<phantom K: copy + drop, phantom V> has key {
        t: T::Table<K, V>
    }

    struct Balance has store {
        value: u128
    }

    #[test]
    fun simple_read_write() {
        let t = T::new<u64, u64>();
        T::add(&mut t, 1, 2);
        T::add(&mut t, 10, 33);
        assert!(*T::borrow(&t, 1) == 2, 1);
        assert!(*T::borrow(&t, 10) == 33, 1);
        T::drop_unchecked(t)
    }

    #[test]
    fun simple_update() {
        let t = T::new<u64, u64>();
        T::add(&mut t, 1, 2);
        assert!(*T::borrow(&t, 1) == 2, 1);
        *T::borrow_mut(&mut t, 1) = 3;
        assert!(*T::borrow(&t, 1) == 3, 1);
        T::drop_unchecked(t)
    }

    #[test]
    fun test_destroy() {
        let t = T::new<u64, u64>();
        T::add(&mut t, 1, 2);
        assert!(*T::borrow(&t, 1) == 2, 1);
        T::remove(&mut t, 1);
        T::destroy_empty(t)
    }

    #[test]
    #[expected_failure(abort_code = 26113, location = extensions::table)]
    fun test_destroy_fails() {
        let t = T::new<u64, u64>();
        T::add(&mut t, 1, 2);
        assert!(*T::borrow(&t, 1) == 2, 1);
        T::destroy_empty(t) // expected to fail
    }

    #[test]
    fun test_length() {
        let t = T::new<u64, u64>();
        T::add(&mut t, 1, 2);
        T::add(&mut t, 2, 2);
        assert!(T::length(&t) == 2, 1);
        T::remove(&mut t, 1);
        assert!(T::length(&t) == 1, 2);
        T::drop_unchecked(t)
    }

    #[test(s = @0x42)]
    fun test_primitive(s: signer) acquires S {
        let t = T::new<u64, u128>();
        assert!(!T::contains(&t, 42), 100);

        T::add(&mut t, 42, 1012);
        assert!(T::contains(&t, 42), 101);
        assert!(!T::contains(&t, 0), 102);
        assert!(*T::borrow(&t, 42) == 1012, 103);

        T::add(&mut t, 43, 1013);
        assert!(T::contains(&t, 42), 104);
        assert!(!T::contains(&t, 0), 105);
        assert!(T::contains(&t, 43), 106);
        assert!(*T::borrow(&t, 43) == 1013, 107);

        let v = T::remove(&mut t, 42);
        assert!(v == 1012, 108);

        move_to(&s, S { t });

        let t_ref = &borrow_global<S<u64, u128>>(@0x42).t;
        let v = *T::borrow(t_ref, 43);
        assert!(v == 1013, 110);

        let S { t: local_t } = move_from<S<u64, u128>>(@0x42);
        assert!(*T::borrow(&local_t, 43) == 1013, 111);

        move_to(&s, S { t: local_t });
    }

    #[test(s = @0x42)]
    fun test_vector(s: signer) acquires S {
        let t = T::new<u8, vector<address>>();

        T::add(&mut t, 42, vector::singleton<address>(@0x1012));
        assert!(T::contains(&t, 42), 101);
        assert!(!T::contains(&t, 0), 102);
        assert!(vector::length(T::borrow(&t, 42)) == 1, 103);
        assert!(*vector::borrow(T::borrow(&t, 42), 0) == @0x1012, 104);

        move_to(&s, S { t });

        let s = borrow_global_mut<S<u8, vector<address>>>(@0x42);
        let v_mut_ref = T::borrow_mut(&mut s.t, 42);
        vector::push_back(v_mut_ref, @0x1013);
        assert!(vector::length(T::borrow(&s.t, 42)) == 2, 105);
        assert!(*vector::borrow(T::borrow(&s.t, 42), 1) == @0x1013, 106);

        let v = T::remove(&mut s.t, 42);
        assert!(vector::length(&v) == 2, 107);
        assert!(*vector::borrow(&v, 0) == @0x1012, 108);
        assert!(*vector::borrow(&v, 1) == @0x1013, 109);
        assert!(!T::contains(&s.t, 42), 110);
    }

    #[test(s = @0x42)]
    fun test_struct(s: signer) acquires S {
        let t = T::new<address, Balance>();
        let val_1 = 11;
        let val_2 = 45;

        T::add(&mut t, @0xAB, Balance{ value: val_1 });
        assert!(T::contains(&t, @0xAB), 101);
        assert!(*&T::borrow(&t, @0xAB).value == val_1, 102);

        move_to(&s, S { t });

        let global_t = &mut borrow_global_mut<S<address, Balance>>(@0x42).t;

        T::add(global_t, @0xCD, Balance{ value: val_2 });
        assert!(*&T::borrow(global_t, @0xAB).value == val_1, 103);
        assert!(*&T::borrow(global_t, @0xCD).value == val_2, 104);


        let entry_mut_ref = T::borrow_mut(global_t , @0xCD);
        *&mut entry_mut_ref.value = entry_mut_ref.value - 1;
        assert!(*&T::borrow(global_t, @0xCD).value == val_2 - 1, 105);

        let Balance { value } = T::remove(global_t, @0xAB);
        assert!(value == val_1, 106);
        assert!(!T::contains(global_t, @0xAB), 107);
    }

    #[test(s = @0x42)]
    fun test_table_of_tables(s: signer) {
        let t = T::new<address, T::Table<address, u128>>();
        let val_1 = 11;
        let val_2 = 45;
        let val_3 = 78;

        // Create two small tables
        let t1 = T::new<address, u128>();
        T::add(&mut t1, @0xAB, val_1);

        let t2 = T::new<address, u128>();
        T::add(&mut t2, @0xCD, val_2);

        // Insert two small tables into the big table
        T::add(&mut t, @0x12, t1);
        T::add(&mut t, @0x34, t2);


        assert!(T::contains(T::borrow(&t, @0x12), @0xAB), 101);
        assert!(T::contains(T::borrow(&t, @0x34), @0xCD), 102);
        assert!(*T::borrow(T::borrow(&t, @0x12), @0xAB) == val_1, 103);
        assert!(*T::borrow(T::borrow(&t, @0x34), @0xCD) == val_2, 104);

        T::add(T::borrow_mut(&mut t, @0x12), @0xEF, val_3);
        assert!(*T::borrow(T::borrow(&t, @0x12), @0xEF) == val_3, 105);
        assert!(*T::borrow(T::borrow(&t, @0x12), @0xAB) == val_1, 106);

        let val = T::remove(T::borrow_mut(&mut t, @0x34), @0xCD);
        assert!(val == val_2, 107);
        assert!(!T::contains(T::borrow(&t, @0x34), @0xCD), 108);

        move_to(&s, S { t });
    }

    #[test(s = @0x42)]
    #[expected_failure(abort_code = 25607, location = extensions::table)]
    fun test_insert_fail(s: signer) {
        let t = T::new<u64, u128>();
        assert!(!T::contains(&t, 42), 100);

        T::add(&mut t, 42, 1012);
        assert!(T::contains(&t, 42), 101);
        T::add(&mut t, 42, 1013); // should fail here since key 42 already exists

        move_to(&s, S { t });
    }

    #[test(s = @0x42)]
    #[expected_failure(abort_code = 25863, location = extensions::table)]
    fun test_borrow_fail(s: signer) {
        let t = T::new<u64, u128>();
        assert!(!T::contains(&t, 42), 100);

        let entry_ref = T::borrow_mut(&mut t, 42); // should fail here since key 42 doesn't exist
        *entry_ref = 1;

        move_to(&s, S { t });
    }

    #[test(s = @0x42)]
    #[expected_failure(abort_code = 25863, location = extensions::table)]
    fun test_remove_fail(s: signer) {
        let t = T::new<u64, Balance>();
        let Balance { value } = T::remove(&mut t, 42); // should fail here since key 42 doesn't exist
        assert!(value == 0, 101);
        move_to(&s, S { t });
    }

    #[test]
    fun test_add_after_remove() {
        let t = T::new<u64, u64>();
        T::add(&mut t, 42, 42);
        let forty_two = T::remove(&mut t, 42);
        assert!(forty_two == 42, 101);

        T::add(&mut t, 42, 0);
        let zero = T::borrow(&mut t, 42);
        assert!(*zero == 0, 102);

        T::drop_unchecked(t)
    }

    #[test]
    #[expected_failure(abort_code = 25863, location = extensions::table)]
    fun test_remove_removed() {
        let t = T::new<u64, u64>();
        T::add(&mut t, 42, 42);
        let forty_two = T::remove(&mut t, 42);
        assert!(forty_two == 42, 101);

        // remove removed value
        let _r = T::remove(&mut t, 42);

        T::drop_unchecked(t)
    }

    #[test]
    #[expected_failure(abort_code = 25863, location = extensions::table)]
    fun test_borrow_removed() {
        let t = T::new<u64, u64>();
        T::add(&mut t, 42, 42);
        let forty_two = T::remove(&mut t, 42);
        assert!(forty_two == 42, 101);

        // borrow removed value
        let _r = T::borrow(&mut t, 42);

        T::drop_unchecked(t)
    }
}
