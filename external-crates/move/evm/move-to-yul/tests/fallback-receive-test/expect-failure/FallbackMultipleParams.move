#[evm_contract]
module 0x2::M {

    #[fallback]
	fun fallback(x: u64, y: u64): u64 {
		x + y
	}

}
