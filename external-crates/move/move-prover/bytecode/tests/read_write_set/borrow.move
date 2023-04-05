// dep: ../../move-stdlib/sources/vector.move

module 0x1::Borrow {
    // ensure that borrows get counted as reads when appropriate
    use std::vector;

    struct S has key { }

    // expected: read a/S
    fun borrow_s(a: address) acquires S {
        _ = borrow_global<S>(a)
    }

    // expected: read a/S
    fun borrow_s_mut(a: address) acquires S {
        _ = borrow_global_mut<S>(a)
    }

    // expected: read v/size
    fun borrow_vec(v: &vector<u64>) {
        let _ = vector::borrow(v, 7);
    }

    // expected: read v/size
    fun borrow_vec_mut(v: &mut vector<u64>) {
        let _ = vector::borrow_mut<u64>(v, 7);
    }
}
