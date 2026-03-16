// Tests that #[allow(unused_let_mut)] on a macro suppresses the pre-expansion
// unused_let_mut warning at the definition site.

module a::m {
    #[allow(unused_let_mut)]
    macro fun suppressed(): u64 {
        let mut x = 5u64;
        x + 1
    }

    fun call_it(): u64 {
        suppressed!()
    }
}
