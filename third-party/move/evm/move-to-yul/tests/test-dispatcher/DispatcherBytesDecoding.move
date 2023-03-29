#[evm_contract]
module 0x2::M {
    use std::vector;

    // Semantic tests for decoding bytes

    // bytes4
    #[callable(sig=b"test_static_bytes_len(bytes4) returns (uint8)")]
    fun test_static_bytes_length(v: vector<u8>): u64 {
        vector::length(&v)
    }

    // bytes32
    #[callable(sig=b"test_static_bytes_last_elem(bytes32) returns (uint8)")]
    fun test_static_bytes_last_element(v: vector<u8>): u8 {
        *vector::borrow(&v, vector::length(&v) - 1)
    }

    // bytes
    #[callable(sig=b"test_dynamic_bytes_len(bytes) returns (uint8)")]
    fun test_dynamic_bytes_length(v: vector<u8>): u64 {
        vector::length(&v)
    }

    // bytes
    #[callable(sig=b"test_dynamic_bytes_last_elem(bytes) returns (uint8)")]
    fun test_dynamic_bytes_last_elem(v: vector<u8>): u8 {
        *vector::borrow(&v, vector::length(&v) - 1)
    }

    // bytes5[2][]
    #[callable(sig=b"test_bytes5_2_dynamic_size_sum(bytes5[2][]) returns (uint64, uint8)")]
    fun test_bytes5_2_dynamic_size_sum(v: vector<vector<vector<u8>>>): (u64, u8) {
        let len_v = vector::length(&v);
        let sum = 0;
        let sum_len = 0;
        let i = 0;
        while (i < len_v) {
            let vec = vector::borrow(&v, i);
            let len_vec = vector::length(vec);
            let j = 0;
            while (j < len_vec) {
                let vec_bytes = vector::borrow(vec, j);
                let len_vec_bytes = vector::length(vec_bytes);
                let v = *vector::borrow(vec_bytes, len_vec_bytes - 1);
                sum_len  = sum_len + len_vec_bytes;
                sum = sum + v;
                j = j + 1;
            };
            i = i + 1;
        };
        (sum_len, sum)
    }

    // string
    #[callable(sig=b"test_string(string) returns (uint64, uint8)")]
    fun test_string(v: vector<u8>) : (u64, u8) {
        let len_str = vector::length(&v);
        (len_str, *vector::borrow(&v, len_str - 1))
    }


}
