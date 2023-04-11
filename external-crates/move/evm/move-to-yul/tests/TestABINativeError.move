#[evm_contract]
module 0x2::M {
    use std::vector;

    // Test type checking on arguments and return values

    #[decode]
    public native fun decode_wrong_input_type(input: vector<u64>): (u8, u8);

    #[decode(sig=b"decode(uint64[]) returns (uint8, uint8)")]
    public native fun decode_wrong_input_type_sig(input: vector<u64>): (u8, u8);

    #[encode]
    public native fun encode_wrong_input_type(input: vector<u64>): vector<u64>;

    #[encode(sig=b"encode(uint64[]) returns (uint64[])")]
    public native fun encode_wrong_input_type_sig(input: vector<u64>): vector<u64>;

    #[encode_packed]
    public native fun encode_wrong_input_type_packed(input: vector<u64>): vector<u64>;

    #[encode_packed(sig=b"encode_packed(uint64[]) returns (uint64[])")]
    public native fun encode_wrong_input_type_packed_sig(input: vector<u64>): vector<u64>;


    #[evm_test]
    fun test_decode_error() {
        let v = vector::empty<u64>();
        vector::push_back(&mut v, 42);
        decode_wrong_input_type(v);
    }

    #[evm_test]
    fun test_decode_sig_error() {
        let v = vector::empty<u64>();
        vector::push_back(&mut v, 42);
        decode_wrong_input_type_sig(v);
    }

    #[evm_test]
    fun test_encode_error() {
        let v = vector::empty<u64>();
        vector::push_back(&mut v, 42);
        encode_wrong_input_type(v);
    }

    #[evm_test]
    fun test_encode_sig_error() {
        let v = vector::empty<u64>();
        vector::push_back(&mut v, 42);
        encode_wrong_input_type_sig(v);
    }

    #[evm_test]
    fun test_encode_packed_error() {
        let v = vector::empty<u64>();
        vector::push_back(&mut v, 42);
        encode_wrong_input_type_packed(v);
    }

    #[evm_test]
    fun test_encode_packed_sig_error() {
        let v = vector::empty<u64>();
        vector::push_back(&mut v, 42);
        encode_wrong_input_type_packed_sig(v);
    }

}
