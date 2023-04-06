#[evm_contract]
module 0x2::M {
    use std::vector;

    // Semantic tests for encoding arrays with multiple return values

    // uint8, uint8[2]
    #[callable(sig=b"test_u8_array_2_uint64(uint8[2],uint64) returns (uint8, uint8[2])")]
    fun test_u8_array_2_uint_64(v: vector<u8>, idx: u64): (u8, vector<u8>) {
        (*vector::borrow(&v, idx), v)
    }

    // uint8,uint8[3],uint8
    #[callable(sig=b"test_uint64_u8_array_3_uint64(uint64,uint8[3],uint64) returns (uint8,uint8[3],uint8)")]
    fun test_u8_uint_64_array_2(idx_1: u64, v: vector<u8>, idx_2: u64): (u8, vector<u8>, u8) {
        (*vector::borrow(&v, idx_1), v, *vector::borrow(&v, idx_2))
    }

    // string, string
    #[callable(sig=b"test_two_strings(string,string) returns (string, string)")]
    fun test_string_length_sum(str_1: vector<u8>, str_2: vector<u8>): (vector<u8>, vector<u8>) {
        (str_2, str_1)
    }

    // uint8[2], bytes
    #[callable(sig=b"test_bytes_string(bytes, uint8[2], string, uint8[2]) returns (uint8[2], bytes)")]
    fun test_bytes_string(str_1: vector<u8>, _array_1: vector<u8>, _str_2: vector<u8>, array_2: vector<u8>): (vector<u8>, vector<u8>) {
        (array_2, str_1)
    }

    // string, uint8
    #[callable(sig=b"test_string_uint8(string) returns (string, uint8)")]
    fun test_string_uint8(v: vector<u8>) : (vector<u8>, u8) {
        let len_str = vector::length(&v);
        (v, *vector::borrow(&v, len_str - 1))
    }


}
