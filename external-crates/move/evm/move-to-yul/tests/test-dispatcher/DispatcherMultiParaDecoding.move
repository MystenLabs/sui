#[evm_contract]
module 0x2::M {
    use std::vector;

    // Semantic tests for decoding arrays with multiple parameters

    // uint8[2],uint64
    #[callable(sig=b"test_u8_array_2_uint64(uint8[2],uint64) returns (uint8)")]
    fun test_u8_array_2_uint_64(v: vector<u8>, idx: u64): u8 {
        *vector::borrow(&v, idx)
    }

    // uint64,uint8[3],uint64
    #[callable(sig=b"test_uint64_u8_array_3_uint64(uint64,uint8[3],uint64) returns (uint8,uint8)")]
    fun test_u8_uint_64_array_2(idx_1: u64, v: vector<u8>, idx_2: u64): (u8, u8) {
        (*vector::borrow(&v, idx_1), *vector::borrow(&v, idx_2))
    }

    // uint8[2], uint64, uint8[2], uint64
    #[callable(sig=b"test_u8_array_uint64_u8_array_uint64(uint8[2], uint64, uint8[2], uint64) returns (uint8)")]
    fun test_u8_array_uint64_u8_array_uint64(v_1: vector<u8>, idx_1: u64, v_2: vector<u8>, idx_2: u64): u8 {
        *vector::borrow(&v_1, idx_1) + *vector::borrow(&v_2, idx_2)
    }

    // string,string
    #[callable(sig=b"test_string_length_sum(string,string) returns (uint64)")]
    fun test_string_length_sum(str_1: vector<u8>, str_2: vector<u8>): u64 {
        vector::length(&str_1) + vector::length(&str_2)
    }

    // string, uint8[2], string, uint8[2]
    #[callable(sig=b"test_static_array_string_static_array_string(string, uint8[2], string, uint8[2]) returns (uint64, uint8)")]
    fun test_static_array_string_static_array_string(str_1: vector<u8>, array_1: vector<u8>, str_2: vector<u8>, array_2: vector<u8>): (u64, u8) {
        let len_str_1 = vector::length(&str_1);
        let len_str_2 = vector::length(&str_2);
        let len_array_1 = vector::length(&array_1);
        let len_array_2 = vector::length(&array_2);
        let len_sum = len_str_1 + len_str_2 + len_array_1 + len_array_2;
        let val_1 = *vector::borrow(&str_1, len_str_1 - 1);
        let val_2 = *vector::borrow(&str_2, len_str_2 - 1);
        let val_3 = *vector::borrow(&array_1, len_array_1 - 1);
        let val_4 = *vector::borrow(&array_2, len_array_2 - 1);
        let val_sum = val_1 + val_2 + val_3 + val_4;
        (len_sum, val_sum)
    }

}
