#[evm_contract]
module 0x2::Tables {
    use Evm::Evm::sign;
    use Evm::Table::{Self, Table};
    use Evm::U256::{u256_from_words, U256, sub, zero, one};
    use std::vector;

    struct S<phantom K, phantom V> has key {
        t: Table<K, V>
    }

    struct Balance has store {
        value: U256
    }

    #[evm_test]
    fun test_primitive() acquires S {
        let t = Table::empty<u64, u128>();
        assert!(!Table::contains(&t, &42), 100);

        Table::insert(&mut t, &42, 1012);
        assert!(Table::contains(&t, &42), 101);
        assert!(!Table::contains(&t, &0), 102);
        assert!(*Table::borrow(&t, &42) == 1012, 103);

        Table::insert(&mut t, &43, 1013);
        assert!(Table::contains(&t, &42), 104);
        assert!(!Table::contains(&t, &0), 105);
        assert!(Table::contains(&t, &43), 106);
        assert!(*Table::borrow(&t, &43) == 1013, 107);

        let v = Table::remove(&mut t, &42);
        assert!(v == 1012, 108);

        move_to(&sign(@0x42), S { t });

        let t_ref = &borrow_global<S<u64, u128>>(@0x42).t;
        assert!(!Table::contains(t_ref, &42), 109);
        let v = *Table::borrow(t_ref, &43);
        assert!(v == 1013, 110);

        let S { t: local_t } = move_from<S<u64, u128>>(@0x42);
        assert!(*Table::borrow(&local_t, &43) == 1013, 111);

        move_to(&sign(@0x43), S { t: local_t });
    }

    #[evm_test]
    fun test_vector() acquires S {
        let t = Table::empty<u8, vector<address>>();

        Table::insert(&mut t, &42, vector::singleton<address>(@0x1012));
        assert!(Table::contains(&t, &42), 101);
        assert!(!Table::contains(&t, &0), 102);
        assert!(vector::length(Table::borrow(&t, &42)) == 1, 103);
        assert!(*vector::borrow(Table::borrow(&t, &42), 0) == @0x1012, 104);

        move_to(&sign(@0x42), S { t });

        let s = borrow_global_mut<S<u8, vector<address>>>(@0x42);
        let v_mut_ref = Table::borrow_mut(&mut s.t, &42);
        vector::push_back(v_mut_ref, @0x1013);
        assert!(vector::length(Table::borrow(&s.t, &42)) == 2, 105);
        assert!(*vector::borrow(Table::borrow(&s.t, &42), 1) == @0x1013, 106);

        let v = Table::remove(&mut s.t, &42);
        assert!(vector::length(&v) == 2, 107);
        assert!(*vector::borrow(&v, 0) == @0x1012, 108);
        assert!(*vector::borrow(&v, 1) == @0x1013, 109);
        assert!(!Table::contains(&s.t, &42), 110);
    }

    #[evm_test]
    fun test_u256() {
        let t = Table::empty<U256, U256>();
        let key = u256_from_words(78, 79);
        let val_1 = u256_from_words(11, 12);
        let val_2 = u256_from_words(45, 46);

        Table::insert(&mut t, &key, val_1);
        assert!(Table::contains(&t, &key), 101);
        assert!(*Table::borrow(&t, &key) == u256_from_words(11, 12), 102);

        let entry_mut_ref = Table::borrow_mut(&mut t, &key);
        *entry_mut_ref = val_2;
        assert!(*Table::borrow(&t, &key) == val_2, 103);

        move_to(&sign(@0x42), S { t });
    }

    #[evm_test]
    fun test_struct() acquires S {
        let t = Table::empty<address, Balance>();
        let val_1 = u256_from_words(11, 12);
        let val_2 = u256_from_words(45, 46);

        Table::insert(&mut t, &@0xAB, Balance{ value: val_1 });
        assert!(Table::contains(&t, &@0xAB), 101);
        assert!(*&Table::borrow(&t, &@0xAB).value == val_1, 102);

        move_to(&sign(@0x42), S { t });

        let global_t = &mut borrow_global_mut<S<address, Balance>>(@0x42).t;

        Table::insert(global_t, &@0xCD, Balance{ value: val_2 });
        assert!(*&Table::borrow(global_t, &@0xAB).value == val_1, 103);
        assert!(*&Table::borrow(global_t, &@0xCD).value == val_2, 104);


        let entry_mut_ref = Table::borrow_mut(global_t , &@0xCD);
        *&mut entry_mut_ref.value = sub(entry_mut_ref.value, one());
        assert!(*&Table::borrow(global_t, &@0xCD).value == u256_from_words(45, 45), 105);

        let Balance { value } = Table::remove(global_t, &@0xAB);
        assert!(value == val_1, 106);
        assert!(!Table::contains(global_t, &@0xAB), 107);
    }

    #[evm_test]
    fun test_table_of_tables() {
        let t = Table::empty<address, Table<address, U256>>();
        let val_1 = u256_from_words(11, 12);
        let val_2 = u256_from_words(45, 46);
        let val_3 = u256_from_words(78, 79);

        // Create two small tables
        let t1 = Table::empty<address, U256>();
        Table::insert(&mut t1, &@0xAB, val_1);

        let t2 = Table::empty<address, U256>();
        Table::insert(&mut t2, &@0xCD, val_2);

        // Insert two small tables into the big table
        Table::insert(&mut t, &@0x12, t1);
        Table::insert(&mut t, &@0x34, t2);


        assert!(Table::contains(Table::borrow(&t, &@0x12), &@0xAB), 101);
        assert!(Table::contains(Table::borrow(&t, &@0x34), &@0xCD), 102);
        assert!(*Table::borrow(Table::borrow(&t, &@0x12), &@0xAB) == val_1, 103);
        assert!(*Table::borrow(Table::borrow(&t, &@0x34), &@0xCD) == val_2, 104);

        Table::insert(Table::borrow_mut(&mut t, &@0x12), &@0xEF, val_3);
        assert!(*Table::borrow(Table::borrow(&t, &@0x12), &@0xEF) == val_3, 105);
        assert!(*Table::borrow(Table::borrow(&t, &@0x12), &@0xAB) == val_1, 106);

        let val = Table::remove(Table::borrow_mut(&mut t, &@0x34), &@0xCD);
        assert!(val == val_2, 107);
        assert!(!Table::contains(Table::borrow(&t, &@0x34), &@0xCD), 108);

        move_to(&sign(@0x42), S { t });
    }

    #[evm_test]
    fun test_insert_fail() {
        let t = Table::empty<u64, u128>();
        assert!(!Table::contains(&t, &42), 100);

        Table::insert(&mut t, &42, 1012);
        assert!(Table::contains(&t, &42), 101);
        Table::insert(&mut t, &42, 1013); // should fail here since key 42 already exists

        move_to(&sign(@0x42), S { t });
    }

    #[evm_test]
    fun test_borrow_fail() {
        let t = Table::empty<u64, u128>();
        assert!(!Table::contains(&t, &42), 100);

        let entry_ref = Table::borrow_mut(&mut t, &42); // should fail here since key 42 doesn't exist
        *entry_ref = 1;

        move_to(&sign(@0x42), S { t });
    }

    #[evm_test]
    fun test_remove_fail() {
        let t = Table::empty<u64, Balance>();
        let Balance { value } = Table::remove(&mut t, &42); // should fail here since key 42 doesn't exist
        assert!(value == zero(), 101);
        move_to(&sign(@0x42), S { t });
    }

}
