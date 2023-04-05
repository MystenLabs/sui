#[evm_contract]
module 0x2::M {
    use Evm::U256::U256;

    // This file contains tests that pass compilation.

    // No parameter and no return value
    #[callable(sig=b"no_para_return(   )")]
    fun no_para_return() {

    }

    // One parameter and one return value
    #[callable(sig=b"$_fun1$   (uint64)returns (int64)")]
    fun fun_1(x: u64) : u64 {
        x
    }

    // Multiple parameter and multiple return values
    // Compatibility between integer / unsigned integer types in Move and Solidity
    #[callable(sig=b"add( int192,uint32 ) returns (int256, int24)")]
    fun f1(x: U256, y: u64): (U256, u64) {
        (x, y)
    }

    // Compatibility between address/signer in Move and Solidity
    #[callable(sig=b"fun_address (address, address ) returns (address payable)")]
    fun f2(_signer: signer, addr: address) : address {
        addr
    }

    // Compatibility between vector in Move and array, bytes and string in Solidity
    #[callable(sig=b"fun_2(int120[ 3 ][][5] memory , address payable [],bytes   [2]memory, bytes1 ,bytes32 ) returns (uint64)")]
    fun f3(_vec0: vector<vector<vector<u128>>>, _vec1: vector<address>, _vec2: vector<vector<u8>>, _vec3: vector<u8>, _vec4: vector<u8>): u128 {
        2
    }

    // Compatibility of Solidity string
    #[callable(sig=b"f(string, string)")]
    fun f4(_s: vector<u8>, _s1:vector <u8> ) {
    }

}
