#[evm_contract]
module 0x2::M {

    #[receive, payable]
    fun receive() {
    }

    #[fallback]
    fun fallback(x: u64) : u64 {
        x + 1
    }

}
