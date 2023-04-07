// This file verifies the intrinsic Table implementation.
module 0x42::VerifyTable {
    use extensions::table::{Self, Table};
    use extensions::table::{spec_get, spec_len, spec_contains};

    // TODO: test precise aborts behavior of all table functions

    fun add(): Table<u8, u64> {
        let t = table::new<u8, u64>();
        table::add(&mut t, 1, 2);
        table::add(&mut t, 2, 3);
        table::add(&mut t, 3, 4);
        t
    }
    spec add {
        ensures spec_contains(result, 1) && spec_contains(result, 2) && spec_contains(result, 3);
        ensures spec_len(result) == 3;
        ensures spec_get(result, 1) == 2;
        ensures spec_get(result, 2) == 3;
        ensures spec_get(result, 3) == 4;
    }

    fun add_fail(): Table<u8, u64> {
        let t = table::new<u8, u64>();
        table::add(&mut t, 1, 2);
        table::add(&mut t, 2, 3);
        table::add(&mut t, 3, 4);
        t
    }
    spec add_fail {
        ensures spec_get(result, 1) == 1;
    }

    fun add_fail_exists(k1: u8, k2: u8): Table<u8, u64> {
        let t = table::new<u8, u64>();
        table::add(&mut t, k1, 2);
        table::add(&mut t, k2, 3);
        t
    }
    spec add_fail_exists {
        aborts_if k1 == k2;
    }

    fun remove(): Table<u8, u64> {
        let t = add();
        table::remove(&mut t, 2);
        t
    }
    spec remove {
        ensures spec_contains(result, 1) && spec_contains(result, 3);
        ensures spec_len(result) == 2;
        ensures spec_get(result, 1) == 2;
        ensures spec_get(result, 3) == 4;
    }

    fun contains_and_length(): (bool, bool, u64, Table<u8, u64>) {
        let t = table::new<u8, u64>();
        table::add(&mut t, 1, 2);
        table::add(&mut t, 2, 3);
        (table::contains(&t, 1), table::contains(&t, 3), table::length(&t), t)
    }
    spec contains_and_length {
        ensures result_1 == true;
        ensures result_2 == false;
        ensures result_3 == 2;
    }

    fun borrow(): (u64, Table<u8, u64>) {
        let t = table::new<u8, u64>();
        table::add(&mut t, 1, 2);
        let r = table::borrow(&t, 1);
        (*r, t)
    }
    spec borrow {
        ensures result_1 == 2;
        ensures spec_len(result_2) == 1;
        ensures spec_get(result_2, 1) == 2;
    }

    fun borrow_mut(): Table<u8, u64> {
        let t = table::new<u8, u64>();
        table::add(&mut t, 1, 2);
        table::add(&mut t, 2, 3);
        let r = table::borrow_mut(&mut t, 1);
        *r = 4;
        t
    }
    spec borrow_mut {
        ensures spec_contains(result, 1) && spec_contains(result, 2);
        ensures spec_len(result) == 2;
        ensures spec_get(result, 1) == 4;
        ensures spec_get(result, 2) == 3;
    }

    // ====================================================================================================
    // Tables with structured keys

    struct Key has copy, drop {
        v: vector<u8>       // Use a vector so we do not have extensional equality
    }

    struct R {
        t: Table<Key, u64>
    }

    fun make_R(): R {
        let t = table::new<Key, u64>();
        table::add(&mut t, Key{v: vector[1, 2]}, 22);
        table::add(&mut t, Key{v: vector[2, 3]}, 23);
        R{t}
    }

    fun add_R(): R {
        make_R()
    }
    spec add_R {
        let k1 = Key{v: concat(vec(1u8), vec(2u8))};
        let k2 = Key{v: concat(vec(2u8), vec(3u8))};
        ensures spec_len(result.t) == 2;
        ensures spec_contains(result.t, k1) && spec_contains(result.t, k2);
        ensures spec_get(result.t, k1) == 22;
        ensures spec_get(result.t, k2) == 23;
    }

    fun add_R_fail(): R {
        make_R()
    }
    spec add_R_fail {
        let k1 = Key{v: concat(vec(1u8), vec(2u8))};
        let k2 = Key{v: concat(vec(2u8), vec(3u8))};
        ensures spec_len(result.t) == 2;
        ensures spec_contains(result.t, k1) && spec_contains(result.t, k2);
        ensures spec_get(result.t, k1) == 23;
        ensures spec_get(result.t, k2) == 22;
    }

    fun borrow_mut_R(): R {
        let r = make_R();
        let x = table::borrow_mut(&mut r.t, Key{v: vector[1, 2]});
        *x = *x * 2;
        r
    }
    spec borrow_mut_R {
        let k1 = Key{v: concat(vec(1u8), vec(2u8))};
        let k2 = Key{v: concat(vec(2u8), vec(3u8))};
        ensures spec_len(result.t) == 2;
        ensures spec_get(result.t, k1) == 44;
        ensures spec_get(result.t, k2) == 23;
    }
}
