module 0x1::ConcretizeVector {
    use std::signer;
    use std::vector;

    struct S has key { v: vector<address> }
    struct T has key { f: u64 }

    public entry fun publish(account1: signer, account2: signer) {
        assert!(signer::address_of(&account1) == @0x1, 1);
        assert!(signer::address_of(&account2) == @0x2, 2);
        move_to(&account1, T { f: 1 });
        move_to(&account2, T { f: 2 });

        // There is a T resource at 0x1 and 0x2, but not 0x3
        let addrs = vector::empty();
        vector::push_back(&mut addrs, @0x1);
        vector::push_back(&mut addrs, @0x2);
        vector::push_back(&mut addrs, @0x3);

        move_to(&account1, S { v: addrs });
    }

    public entry fun read_vec(a: address) acquires S, T {
        let addrs = &borrow_global<S>(a).v;
        let addr = *vector::borrow(addrs, 1);
        borrow_global<T>(addr).f;
    }

    public entry fun write_vec(a: address, x: u64) acquires S, T {
        let addrs = &borrow_global<S>(a).v;
        let addr = *vector::borrow(addrs, 1);
        *&mut borrow_global_mut<T>(addr).f = x
    }

}
