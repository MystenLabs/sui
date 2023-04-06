// separate_baseline: simplify
module 0x42::TestGlobalVars {
    use std::signer;

    // ================================================================================
    // Counting

    spec module {
        global sum_of_T: u64 = 0;
    }

    struct T has key {
      i: u64,
    }

    fun add() acquires T {
        borrow_global_mut<T>(@0).i = borrow_global_mut<T>(@0).i + 1;
        spec {
            update sum_of_T = sum_of_T + 1;
        };
    }

    fun sub() acquires T {
        borrow_global_mut<T>(@0).i = borrow_global_mut<T>(@0).i - 1;
        spec {
            update sum_of_T = sum_of_T - 1;
        };
    }

    fun call_add_sub() acquires T {
        add(); add(); sub();
    }
    spec call_add_sub {
        ensures sum_of_T == 1;
    }

    fun call_add_sub_invalid() acquires T {
        add(); sub(); add();
    }
    spec call_add_sub_invalid {
        ensures sum_of_T == 2;
    }

    // ================================================================================
    // Counting (opaque)

    fun opaque_add() acquires T {
        borrow_global_mut<T>(@0).i = borrow_global_mut<T>(@0).i + 1
    }
    spec opaque_add {
        pragma opaque;
        modifies global<T>(@0);
        update sum_of_T = sum_of_T + 1;
    }

    fun opaque_sub() acquires T {
        borrow_global_mut<T>(@0).i = borrow_global_mut<T>(@0).i - 1
    }
    spec opaque_sub {
        pragma opaque;
        modifies global<T>(@0);
        update sum_of_T = sum_of_T - 1;
    }

    fun opaque_call_add_sub() acquires T {
        opaque_add(); opaque_add(); opaque_sub();
    }
    spec opaque_call_add_sub {
        ensures sum_of_T == 1;
    }

    fun opaque_call_add_sub_invalid() acquires T {
        opaque_add(); opaque_sub(); opaque_add();
    }
    spec opaque_call_add_sub_invalid {
        ensures sum_of_T == 2;
    }

    // ================================================================================
    // Access Control

    spec module {
        // Indicates whether a specific access has been verified. This is kept intentionally
        // uninitialized so the prover will find situations where this is false but access is required.
        global access_verified: bool;
    }

    fun assert_access(s: &signer) {
        // Do some assertions which validate access
        assert!(signer::address_of(s) == @0, 1);
    }
    spec assert_access {
        aborts_if signer::address_of(s) != @0;
        update access_verified = true;
    }

    fun requires_access() {
        // Do some things which require access to be validated.
    }
    spec requires_access {
        requires access_verified;
    }

    fun do_privileged(s: &signer) {
        assert_access(s);
        requires_access();
    }

    fun do_privileged_invalid(_s: &signer) {
        requires_access();
    }

    // ================================================================================
    // Generic spec vars

    spec module {
        global type_has_property<X>: bool;
    }

    fun give_property_to<X>() {
    }
    spec give_property_to {
        update type_has_property<X> = true;
    }

    fun expect_property_of_bool() {
        give_property_to<bool>();
    }
    spec expect_property_of_bool {
        ensures type_has_property<bool>;
    }

    fun expect_property_of_u64_invalid() {
        give_property_to<bool>();
    }
    spec expect_property_of_u64_invalid {
        ensures type_has_property<u64>;
    }

    // ================================================================================
    // Generic spec vars (opaque)

    fun opaque_give_property_to<X>() {
    }
    spec opaque_give_property_to {
        pragma opaque;
        update type_has_property<X> = true;
    }

    fun opaque_expect_property_of_bool() {
        opaque_give_property_to<bool>();
    }
    spec opaque_expect_property_of_bool {
        ensures type_has_property<bool>;
    }

    fun opaque_expect_property_of_u64_invalid() {
        opaque_give_property_to<bool>();
    }
    spec opaque_expect_property_of_u64_invalid {
        ensures type_has_property<u64>;
    }


    // ================================================================================
    // Invariants and spec vars

    spec module {
        global limit: num = 2;
    }

    struct R has key { v: u64 }

    invariant global<R>(@0).v <= limit;

    fun publish(s: &signer) {
        move_to<R>(s, R{v: 2});
    }

    fun update_invalid() acquires R {
        borrow_global_mut<R>(@0).v = 3;
    }

    fun limit_change_invalid(s: &signer) {
        publish(s);
    }
    spec limit_change_invalid {
        update limit = 1;
    }
}
