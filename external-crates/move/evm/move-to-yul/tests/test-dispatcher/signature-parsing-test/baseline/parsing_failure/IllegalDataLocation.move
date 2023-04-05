#[evm_contract]
module 0x2::M {

    #[callable(sig=b"f(uint64 memory)")]
    fun primitive_memory(_x:u128) {
    }

    #[callable(sig=b"f() returns (uint64 calldata)")]
    fun primitive_calldata(): u128 {
        0
    }

    #[callable(sig=b"f(bytes32 calldata) returns (uint64)")]
    fun bytes_calldata(_v: vector<u8>): u128 {
        0
    }

}
