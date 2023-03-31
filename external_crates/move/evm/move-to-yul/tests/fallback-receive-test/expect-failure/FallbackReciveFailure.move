#[evm_contract]
module 0x2::M {

    #[fallback, callable, receive]
	fun fallback(x: u64): u64 {
		x
	}

	#[receive, callable, fallback]
	fun receive(x: u64): u64 {
		x
	}


}
