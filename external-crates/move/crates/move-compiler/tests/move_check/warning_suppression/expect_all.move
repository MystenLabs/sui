// Test that #[expect(all)] is rejected — wildcards are not allowed with expect.

#[expect(all)]
module 0x42::m { fun var(a: u64) { } }

// Also test category-level wildcard.
#[expect(unused)]
module 0x42::n { fun foo(a: u64) { } }
