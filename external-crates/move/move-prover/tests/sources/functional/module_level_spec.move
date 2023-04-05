// separate_baseline: simplify
module 0x42::TestModule {

    struct R has key { value: u64 }

    fun store(s: &signer, value: u64) {
       move_to<R>(s, R{value})
    }

    fun store_incorrect(s: &signer, value: u64) {
        move_to<R>(s, R{value})
    }
}

spec 0x42::TestModule {
    use std::signer;

    invariant forall addr: address where exists<R>(addr): global<R>(addr).value > 0;

    spec store(s: &signer, value: u64) {
        requires value > 0;
        include Store;
    }

    spec store_incorrect(s: &signer, value: u64) {
        include Store;
    }

    spec schema Store {
        s: signer;
        value: u64;
        let addr = signer::address_of(s);
        ensures exists<R>(addr);
        ensures global<R>(addr).value == value;
    }
}
