module 0x42::Module {
    public struct S { i: u64 }

    public fun foo(i: u64): S {
        S { i }
    }
}
