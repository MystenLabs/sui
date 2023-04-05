module 0x42::Test {
    struct S1 has key, store, drop { v: u64 }

    struct S2<T: store + drop> has key, store, drop { t: T }

    fun test1(account: signer) {
        move_to(&account, S1 { v: 0 });
        // invariant is related here and verifies
    }

    fun test2(account: address) acquires S1 {
        move_from<S1>(account);
        // invariant is related here and does not verify
    }

    fun test3(account: address) acquires S1 {
        let s1 = borrow_global_mut<S1>(account);
        *&mut s1.v = 0;
        // invariant is related here and verifies
    }

    fun test4(account: address) acquires S1 {
        let s1 = borrow_global_mut<S1>(account);
        *&mut s1.v = 0;
        // invariant is related here and verifies

        let s1 = borrow_global_mut<S1>(account);
        *&mut s1.v = 1;
        // invariant is related here and verifies
    }

    spec module {
        invariant<T> exists<S2<T>>(@0x42) ==> exists<S1>(@0x42);
    }
}
