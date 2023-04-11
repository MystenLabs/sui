#[contract]
module 0x1::M {

    #[callable]
    fun add(x: u64): u64 { x + 1 }

    fun expect_can_call(x: u64): u64 {
        0x1::M::call_add(@1, x) + call_add(@2, x)
    }

    #[callable]
    fun expect_duplicate(x: u64): u64 { x + 1 }

    fun call_expect_duplicate(_c: address) { }
}
