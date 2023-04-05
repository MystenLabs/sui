
// separate_baseline: simplify
// The separate baseline is legit and caused by a different choice in the generated model.
module 0x42::SimpleIsTxnSigner {
    use std::signer;

    // ----------------------------------------
    // Simple examples for `is_txn_signer`
    // ----------------------------------------


    // ----------------------------------------
    // Simple examples for `is_txn_signer_addr`
    // ----------------------------------------

    public fun f1_incorrect() {
        spec { assert signer::is_txn_signer_addr(@0x7); } // This is unprovable because it is not true in general.
    }

    public fun f2_incorrect(_account: &signer) {
        spec { assert signer::is_txn_signer_addr(@0x7); } // This is unprovable because it is not true in general.
    }

    public fun f3(account: &signer) {
        assert!(signer::address_of(account) == @0x7, 1);
        spec { assert signer::is_txn_signer_addr(@0x7); } // It's true in general.
    }

    public fun f4_incorrect(account: &signer) {
        assert!(signer::address_of(account) == @0x3, 1);
        spec { assert signer::is_txn_signer_addr(@0x7); } // This is unprovable because it is not true in general.
    }

    fun f5() {
        spec { assert signer::is_txn_signer_addr(@0x7); } // This is provable because of the "requires" condition.
    }
    spec f5 {
        requires signer::is_txn_signer_addr(@0x7); // f5 requires this to be true at its callers' sites
    }

    public fun f6(account: &signer) { // This function produces no error because it satisfies f5's requires condition.
        assert!(signer::address_of(account) == @0x7, 1);
        f5();
    }

    public fun f7_incorrect() {
        // This function does not satisfy f5's requires condition.
        f5();
    }



    // -----------------------------------
    // Access control for Counter resource
    // -----------------------------------

    struct Counter has key { i: u64 }

    public fun ADMIN_ADDRESS(): address {
        @0x7
    }

    const AUTH_FAILED: u64 = 1;

    public fun publish(account: &signer) {
        spec { assume signer::is_txn_signer_addr(signer::address_of(account)); };

        assert!(signer::address_of(account) == ADMIN_ADDRESS(), AUTH_FAILED);
        move_to(account, Counter { i: 0 });
    }

    public fun get_counter(): u64 acquires Counter {
        borrow_global<Counter>(ADMIN_ADDRESS()).i
    }

    public fun increment(account: &signer) acquires Counter {
        assert!(signer::address_of(account) == ADMIN_ADDRESS(), AUTH_FAILED); // permission check
        let c_ref = &mut borrow_global_mut<Counter>(ADMIN_ADDRESS()).i;
        *c_ref = *c_ref + 1;
    }

    // This function is incorrect because it omits the permission check.
    public fun increment_incorrect(_account: &signer) acquires Counter {
        let c_ref = &mut borrow_global_mut<Counter>(ADMIN_ADDRESS()).i;
        *c_ref = *c_ref + 1;
    }

    spec module {
        // Access control spec: Only the admin is allowed to increment the counter value.
        invariant update (old(exists<Counter>(ADMIN_ADDRESS())) && global<Counter>(ADMIN_ADDRESS()).i != old(global<Counter>(ADMIN_ADDRESS()).i))
            ==> signer::is_txn_signer_addr(ADMIN_ADDRESS());
    }
}
