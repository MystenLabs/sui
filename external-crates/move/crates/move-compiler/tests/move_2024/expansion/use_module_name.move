module a::vector {
    public fun borrow<T>(_v: &vector<T>, _n: u64): &T { abort 0 }
    public fun length<T>(_v: &vector<T>): u64 { abort 0 }
    public fun singleton<T>(_t: T): vector<T> { abort 0 }
}

module a::vector_tests {
    use a::vector as V;

    public struct R has store { }
    public struct Droppable has drop {}
    public struct NotDroppable {}

    fun test_singleton_contains() {
        assert!(*V::borrow(&V::singleton(0), 0) == 0, 0);
        assert!(*V::borrow(&V::singleton(true), 0) == true, 0);
        assert!(*V::borrow(&V::singleton(@0x1), 0) == @0x1, 0);
    }

    fun test_singleton_len() {
        assert!(V::length(&V::singleton(0)) == 1, 0);
        assert!(V::length(&V::singleton(true)) == 1, 0);
        assert!(V::length(&V::singleton(@0x1)) == 1, 0);
    }
}
