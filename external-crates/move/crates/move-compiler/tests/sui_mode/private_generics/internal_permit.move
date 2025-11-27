// tests that `internal::permit` can only be called by the module that defines the type

module a::m {
    use a::other::NonInternalType;
    use std::internal::{Self, Permit};

    struct Internal {}

    struct InternalGeneric<phantom T> {}

    // success
    public fun t1() {
        internal::permit<Internal>();
    }

    // fail: non-internal type
    public fun t2() {
        internal::permit<NonInternalType>();
    }

    // success: the base type is internal
    public fun t3() {
        internal::permit<InternalGeneric<NonInternalType>>();
    }

    // fail: the base type is not internal
    public fun t4() {
        internal::permit<Permit<Internal>>();
    }

    // fail: the base type is not internal (primitive)
    public fun t5() {
        internal::permit<u64>();
    }

    // fail: the base type is not internal (vector)
    public fun t6() {
        internal::permit<vector<Internal>>();
    }
}

module a::other {
    struct NonInternalType {}
}

module std::internal {
    struct Permit<phantom T> has drop {}

    public fun permit<T>(): Permit<T> { Permit {} }
}
