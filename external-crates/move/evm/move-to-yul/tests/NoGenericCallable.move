// Tests error on generic callable.
#[evm_contract]
module 0x2::M {
    #[callable]
    fun f<T>(x: u64): u64 { x }
}
