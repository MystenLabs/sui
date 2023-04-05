#[evm_contract]
module 0x2::M {
    use Evm::U256::U256;

    // Semantic tests for encoding array types

    // uint8[2]
    #[callable(sig=b"test_u8(uint8[2]) returns (uint8[2])")]
    fun test_array_u8(v: vector<u8>): vector<u8> {
        v
    }

    // uint8[2][2]
    #[callable(sig=b"test_u8(uint8[2][2]) returns (uint8[2][2]) ")]
    fun test_one_elem_para_array_2_array_2_u8(v: vector<vector<u8>>): vector<vector<u8>> {
       v
    }

    // uint8[2][]
    #[callable(sig=b"test_u8(uint8[2][]) returns (uint8[2][])")]
    fun test_one_elem_para_array_dynamic_array_2_u8(v: vector<vector<u8>>): vector<vector<u8>> {
        v
    }

    // uint8[][2]
    #[callable(sig=b"test_u8(uint8[][2]) returns (uint8[][2])")]
    fun test_one_elem_para_array_2_array_dynamic_u8(v: vector<vector<u8>>): vector<vector<u8>> {
        v
    }

    // uint64[2]
   #[callable(sig=b"test_u64(uint64[2]) returns (uint64[2])")]
    fun test_array_u64(v: vector<u64>): vector<u64>{
        v
    }

    // uint128[][]
    #[callable(sig=b"test_u128(uint128[][]) returns (uint128[][])")]
    fun test_one_elem_para_array_dynamic_array_u128(v: vector<vector<u128>>): vector<vector<u128>> {
        v
    }

    // uint256[][2]
    #[callable(sig=b"test_u256(uint[][2]) returns (uint[][2])")]
    fun test_one_elem_para_array_2_array_dynamic_u256(v: vector<vector<U256>>): vector<vector<U256>> {
        v
    }

    // uint72[][2]
    #[callable(sig=b"test_uint72_u128(uint72[][2]) returns (uint72[][2])")]
    fun test_array_uint72_u128(v: vector<vector<u128>>): vector<vector<u128>> {
        v
    }

}
