module 0xa::M1 {
    use std::vector;

    struct S { i: u64 }

    public fun foo(x: u64): vector<u64> {
        let y = bar();
        vector::singleton(x + y)
    }

    public fun bar(): u64 {
        7
    }
}
