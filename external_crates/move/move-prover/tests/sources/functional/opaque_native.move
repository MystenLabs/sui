module 0x42::OpaqueNative {

    spec module {
        pragma aborts_if_is_strict = true;

        // Uninterpreted spec function with axiom.
        fun injection(v: vector<u8>): u64;
        axiom forall v1: vector<u8>, v2: vector<u8>: v1 == v2 <==> injection(v1) == injection(v2);

        // Uninterpreted generic spec function with axiom.
        fun generic_injection<T>(x: T): u64;
        axiom<T> forall v1: T, v2: T: v1 == v2 <==> generic_injection(v1) == generic_injection(v2);

    }

    // Axiomatized native function
    native fun hash(v: vector<u8>): u64;
    spec hash {
        pragma opaque;
        ensures result == injection(v);
    }

    // Positive Test.
    fun test(v1: vector<u8>, v2: vector<u8>) {
        assert!(hash(v1) == hash(v1), 1);
        assert!(v1 == v2 || hash(v1) != hash(v2), 2);
    }

    // Negative Test.
    fun negative_test(v1: vector<u8>, v2: vector<u8>) {
        assert!(hash(v1) != hash(v2), 1);
    }
    spec negative_test {
        aborts_if v1 == v2 with 1;
    }

    native fun hash_generic<T>(x: T): u64;
    spec hash_generic {
        pragma opaque;
        ensures result == generic_injection(x);
    }

    // Positive Test.
    fun test_generic(v1: vector<u8>, v2: vector<u8>) {
        assert!(hash_generic(v1) == hash_generic(v1), 1);
        assert!(v1 == v2 || hash(v1) != hash(v2), 2);
    }

    // Negative Test.
    fun negative_test_generic(v1: vector<u8>, v2: vector<u8>) {
        assert!(hash_generic(v1) != hash_generic(v2), 1);
    }
    spec negative_test_generic {
        aborts_if v1 == v2 with 1;
    }
}
