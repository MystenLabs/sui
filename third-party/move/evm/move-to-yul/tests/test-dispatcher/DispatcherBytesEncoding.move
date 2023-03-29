#[evm_contract]
module 0x2::M {
    use std::vector;

    // Semantic tests for encoding bytes

    // bytes4
    #[callable(sig=b"test_static_bytes(bytes4,uint8) returns (bytes5)")]
    fun test_static_bytes_length(v: vector<u8>, c: u8): vector<u8> {
        vector::push_back(&mut v, c);
        v
    }

    // bytes5[2][]
    #[callable(sig=b"test_bytes5_2_dynamic(bytes5[2][]) returns (bytes5[2][])")]
    fun test_bytes5_2_dynamic(v: vector<vector<vector<u8>>>): vector<vector<vector<u8>>> {
        v
    }

    // bytes
    #[callable(sig=b"test_bytes(bytes) returns (bytes)")]
    fun test_bytes(v: vector<u8>): vector<u8> {
        v
    }

    // string
    #[callable(sig=b"test_string(string) returns (string)")]
    fun test_string(v: vector<u8>) : vector<u8> {
       v
    }

}
