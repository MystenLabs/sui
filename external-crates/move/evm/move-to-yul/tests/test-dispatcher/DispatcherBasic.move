#[evm_contract]
module 0x2::M {
    #[callable]
    fun return_0(): u128 {
        0
	}

    #[callable]
    fun return_1(): u128 {
	    1
    }

    #[callable]
    fun return_2(): u128 {
	    2
    }

}
