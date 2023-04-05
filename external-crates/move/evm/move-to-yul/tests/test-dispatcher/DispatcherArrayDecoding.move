#[evm_contract]
module 0x2::M {
    use std::vector;
    use Evm::U256::{Self, U256};

    // Semantic tests for decoding array types

    // uint8[2]
    #[callable(sig=b"test_u8(uint8[2]) returns (uint8)")]
    fun test_array_u8(v: vector<u8>): u8{
        let val_1 = *vector::borrow(&v, 0);
        let val_2 = *vector::borrow(&v, 1);
        val_1 + val_2
    }

    // uint8[2][2]
    #[callable(sig=b"test_u8(uint8[2][2]) returns (uint8) ")]
    fun test_one_elem_para_array_2_array_2_u8(v: vector<vector<u8>>): u8{
        let vec_1 = vector::borrow(&v, 0);
        let vec_2 = vector::borrow(&v, 1);
        let vec_1_val_1 = *vector::borrow(vec_1, 0);
        let vec_1_val_2 = *vector::borrow(vec_1, 1);
        let vec_2_val_1 = *vector::borrow(vec_2, 0);
        let vec_2_val_2 = *vector::borrow(vec_2, 1);
        vec_1_val_1 + vec_1_val_2 + vec_2_val_1 - vec_2_val_2
    }

    // uint8[2][]
    #[callable(sig=b"test_u8(uint8[2][]) returns (uint8)")]
    fun test_one_elem_para_array_dynamic_array_2_u8(v: vector<vector<u8>>): u8 {
        let len_v = vector::length(&v);
        let i = 0;
        let sum = 0;
        while (i < len_v) {
            let vec = vector::borrow(&v, i);
            let vec_val_1 = *vector::borrow(vec, 0);
            let vec_val_2 = *vector::borrow(vec, 1);
            sum = sum + vec_val_1 + vec_val_2;
            i = i + 1;
        };
        sum
    }

    // uint8[][2]
    #[callable(sig=b"test_u8(uint8[][2]) returns (uint8)")]
    fun test_one_elem_para_array_2_array_dynamic_u8(v: vector<vector<u8>>): u8 {
        let i = 0;
        let sum = 0;
        while (i < 2) {
            let vec = vector::borrow(&v, i);
            let len_vec = vector::length(vec);
            let j = 0;
            while (j < len_vec) {
                let vec_val = *vector::borrow(vec, j);
                sum = sum + vec_val;
                j = j + 1;
            };
            i = i + 1;
        };
        sum
    }

    // uint64[2]
   #[callable(sig=b"test_u64(uint64[2]) returns (uint64)")]
    fun test_array_u64(v: vector<u64>): u64{
        let val_1 = *vector::borrow(&v, 0);
        let val_2 = *vector::borrow(&v, 1);
        val_1 + val_2
    }

    // uint64[2][2]
    #[callable(sig=b"test_u64(uint64[2][2]) returns (uint64) ")]
    fun test_one_elem_para_array_2_array_2_u64(v: vector<vector<u64>>): u64{
        let vec_1 = vector::borrow(&v, 0);
        let vec_2 = vector::borrow(&v, 1);
        let vec_1_val_1 = *vector::borrow(vec_1, 0);
        let vec_1_val_2 = *vector::borrow(vec_1, 1);
        let vec_2_val_1 = *vector::borrow(vec_2, 0);
        let vec_2_val_2 = *vector::borrow(vec_2, 1);
        vec_1_val_1 + vec_1_val_2 + vec_2_val_1 + vec_2_val_2
    }

    // uint64[2][]
    #[callable(sig=b"test_u64(uint64[2][]) returns (uint64)")]
    fun test_one_elem_para_array_dynamic_array_2_u64(v: vector<vector<u64>>): u64 {
        let len_v = vector::length(&v);
        let i = 0;
        let sum = 0;
        while (i < len_v) {
            let vec = vector::borrow(&v, i);
            let vec_val_1 = *vector::borrow(vec, 0);
            let vec_val_2 = *vector::borrow(vec, 1);
            sum = sum + vec_val_1 + vec_val_2;
            i = i + 1;
        };
        sum
    }

    // uint64[][2]
    #[callable(sig=b"test_u64(uint64[][2]) returns (uint64)")]
    fun test_one_elem_para_array_2_array_dynamic_u64(v: vector<vector<u64>>): u64 {
        let i = 0;
        let sum = 0;
        while (i < 2) {
            let vec = vector::borrow(&v, i);
            let len_vec = vector::length(vec);
            let j = 0;
            while (j < len_vec) {
                let vec_val = *vector::borrow(vec, j);
                sum = sum + vec_val;
                j = j + 1;
            };
            i = i + 1;
        };
        sum
    }

    // uint128[][]
    #[callable(sig=b"test_u128(uint128[][]) returns (uint128)")]
    fun test_one_elem_para_array_dynamic_array_u128(v: vector<vector<u128>>): u128 {
        let len_v = vector::length(&v);
        let i = 0;
        let sum = 0;
        while (i < len_v) {
            let vec = vector::borrow(&v, i);
            let j = 0;
            while (j < vector::length(vec)) {
                sum = sum + *vector::borrow(vec, j);
                j = j + 1;
            };
            i = i + 1;
        };
        sum
    }

    // uint256[2][]
    #[callable(sig=b"test_u256(uint256[2][]) returns (uint256)")]
    fun test_one_elem_para_array_dynamic_array_2_u256(v: vector<vector<U256>>): U256 {
        let len_v = vector::length(&v);
        let i = 0;
        let sum = U256::zero();
        while (i < len_v) {
            let vec = vector::borrow(&v, i);
            let vec_val_1 = *vector::borrow(vec, 0);
            let vec_val_2 = *vector::borrow(vec, 1);
            sum = U256::add(sum, U256::add(vec_val_1, vec_val_2));
            i = i + 1;
        };
        sum
    }

    // uint256[][2]
    #[callable(sig=b"test_u256(uint[][2]) returns (uint256)")]
    fun test_one_elem_para_array_2_array_dynamic_u256(v: vector<vector<U256>>): U256 {
        let i = 0;
        let sum = U256::zero();
        while (i < 2) {
            let vec = vector::borrow(&v, i);
            let len_vec = vector::length(vec);
            let j = 0;
            while (j < len_vec) {
                let vec_val = *vector::borrow(vec, j);
                sum = U256::add(sum, vec_val);
                j = j + 1;
            };
            i = i + 1;
        };
        sum
    }

    // Testing type compatibility

    // vector<u64> : uint8[2]
    #[callable(sig=b"test_uint8_u64(uint8[2]) returns (uint8)")]
    fun test_array_uint8_u64(v: vector<u64>): u64 {
        let val_1 = *vector::borrow(&v, 0);
        let val_2 = *vector::borrow(&v, 1);
        val_1 + val_2
    }

    // vector<vector<u64>> : uint16[2][]
    #[callable(sig=b"test_uint16_u64(uint16[2][]) returns (uint16)")]
    fun test_array_uint16_u64(v: vector<vector<u64>>): u64 {
        let len_v = vector::length(&v);
        let i = 0;
        let sum = 0;
        while (i < len_v) {
            let vec = vector::borrow(&v, i);
            let vec_val_1 = *vector::borrow(vec, 0);
            let vec_val_2 = *vector::borrow(vec, 1);
            sum = sum + vec_val_1 + vec_val_2;
            i = i + 1;
        };
        sum
    }

    // vector<vector<u128>> : uint72[][2]
    #[callable(sig=b"test_uint72_u128(uint72[][2]) returns (uint72)")]
    fun test_array_uint72_u128(v: vector<vector<u128>>): u128 {
        let i = 0;
        let sum = 0;
        while (i < 2) {
            let vec = vector::borrow(&v, i);
            let len_vec = vector::length(vec);
            let j = 0;
            while (j < len_vec) {
                let vec_val = *vector::borrow(vec, j);
                sum = sum + vec_val;
                j = j + 1;
            };
            i = i + 1;
        };
        sum
    }

    // vector<vector<U256>> : uint8[2][]
    #[callable(sig=b"test_uint8_U256(uint8[2][]) returns (uint8)")]
    fun test_array_uint8_U256(v: vector<vector<U256>>): U256 {
        let len_v = vector::length(&v);
        let i = 0;
        let sum = U256::zero();
        while (i < len_v) {
            let vec = vector::borrow(&v, i);
            let vec_val_1 = *vector::borrow(vec, 0);
            let vec_val_2 = *vector::borrow(vec, 1);
            sum = U256::add(sum, U256::add(vec_val_1, vec_val_2));
            i = i + 1;
        };
        sum

    }

}
