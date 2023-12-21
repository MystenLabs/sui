// flag: --trace
// separate_baseline: cvc5
// separate_baseline: simplify
module 0x42::TestTracing {
    use std::signer;

    spec module {
        pragma verify = true;
    }

    struct R has key {
        x: u64
    }

    fun add_invalid(a: u64, b: u64): u64 {
        a + b -1
    }
    spec add_invalid {
        ensures result == a + b;
    }

    fun update_invalid(a: &mut u64, b: u64) {
        *a = *a + b - 1
    }
    spec update_invalid {
        ensures a == old(a) + b;
    }

    fun publish_invalid(s: &signer, x: u64) {
        move_to<R>(s, R{x: x - 1})
    }
    spec publish_invalid {
        let addr = signer::address_of(s);
        ensures exists<R>(addr) ==> global<R>(addr).x == x;
    }

    // Test whether auto trace on expressions in quantifiers does not trigger error
    invariant forall addr: address: exists<R>(addr) ==> global<R>(addr).x < 5;
}
