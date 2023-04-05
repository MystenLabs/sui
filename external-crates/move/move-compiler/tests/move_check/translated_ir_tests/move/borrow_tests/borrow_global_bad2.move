module 0x8675309::A {
    use std::signer;
    struct T has key {v: u64}

    public fun A2(account: &signer) acquires T {
        let sender = signer::address_of(account);
        let t_ref = borrow_global_mut<T>(sender);
        T { v: _ } = move_from<T>(sender);
        t_ref;
    }
}

// check: GLOBAL_REFERENCE_ERROR
