// Test that #[expect(all)] is rejected — wildcards are not allowed with expect.
module 0x42::x {}

#[expect(all)]
module 0x42::m {
    struct B<phantom T> {}
    struct S<T1, T2> { f: B<T1> }

    use 0x42::x;
    fun var(a: u64) {
        let x;
    }
}

// Also test category-level wildcard.
#[expect(unused)]
module 0x42::n {
    fun foo(a: u64) {
        let x;
    }
}
