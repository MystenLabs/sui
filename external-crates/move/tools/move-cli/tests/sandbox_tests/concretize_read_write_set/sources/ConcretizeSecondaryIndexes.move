module 0x1::ConcretizeSecondaryIndexes {
    struct Addr has key { a: address }
    struct S has key { f: u64 }

    public entry fun publish_addr(account: signer, a: address) {
        move_to(&account, Addr { a })
    }

    public entry fun publish(account: signer) {
        move_to(&account, S { f: 10 })
    }

    public entry fun read_indirect(a: address) acquires Addr, S {
        let addr = *&borrow_global<Addr>(a).a;
        borrow_global<S>(addr).f;
    }

    public entry fun multi_arg(_s: signer, a: address, _i: u64) acquires Addr, S {
        let addr = *&borrow_global<Addr>(a).a;
        borrow_global<S>(addr).f;
    }
}
