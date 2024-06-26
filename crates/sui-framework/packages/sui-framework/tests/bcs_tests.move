
module sui::bcs_tests {

    use sui::bcs::{Self, to_bytes, new};

    #[test_only]
    public struct Info has drop { a: bool, b: u8, c: u64, d: u128, k: vector<bool>, s: address }

    #[test]
    #[expected_failure(abort_code = bcs::ELenOutOfRange)]
    fun test_uleb_len_fail() {
        let value = vector[0xff, 0xff, 0xff, 0xff, 0xff];
        let mut bytes = new(to_bytes(&value));
        let _fail = bytes.peel_vec_length();
        abort 2 // TODO: make this test fail
    }

    #[test]
    #[expected_failure(abort_code = bcs::ENotBool)]
    fun test_bool_fail() {
        let mut bytes = new(to_bytes(&10u8));
        let _fail = bytes.peel_bool();
    }

    #[test]
    fun test_option() {
        {
            let value = option::some(true);
            let mut bytes = new(to_bytes(&value));
            assert!(value == bytes.peel_option_bool());
        };

        {
            let value = option::some(10u8);
            let mut bytes = new(to_bytes(&value));
            assert!(value == bytes.peel_option_u8());
        };

        {
            let value = option::some(10000u64);
            let mut bytes = new(to_bytes(&value));
            assert!(value == bytes.peel_option_u64());
        };

        {
            let value = option::some(10000999999u128);
            let mut bytes = new(to_bytes(&value));
            assert!(value == bytes.peel_option_u128());
        };

        {
            let value = option::some(@0xC0FFEE);
            let mut bytes = new(to_bytes(&value));
            assert!(value == bytes.peel_option_address());
        };

        {
            let value: Option<bool> = option::none();
            let mut bytes = new(to_bytes(&value));
            assert!(value == bytes.peel_option_bool());
        };
    }

    #[test]
    fun test_bcs() {
        {
            let value = @0xC0FFEE;
            let mut bytes = new(to_bytes(&value));
            assert!(value == bytes.peel_address());
        };

        { // boolean: true
            let value = true;
            let mut bytes = new(to_bytes(&value));
            assert!(value == bytes.peel_bool());
        };

        { // boolean: false
            let value = false;
            let mut bytes = new(to_bytes(&value));
            assert!(value == bytes.peel_bool());
        };

        { // u8
            let value = 100u8;
            let mut bytes = new(to_bytes(&value));
            assert!(value == bytes.peel_u8());
        };

        { // u64 (4 bytes)
            let value = 1000100u64;
            let mut bytes = new(to_bytes(&value));
            assert!(value == bytes.peel_u64());
        };

        { // u64 (8 bytes)
            let value = 100000000000000u64;
            let mut bytes = new(to_bytes(&value));
            assert!(value == bytes.peel_u64());
        };

        { // u128 (16 bytes)
            let value = 100000000000000000000000000u128;
            let mut bytes = new(to_bytes(&value));
            assert!(value == bytes.peel_u128());
        };

        { // vector length
            let value = vector[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0];
            let mut bytes = new(to_bytes(&value));
            assert!(value.length() == bytes.peel_vec_length());
        };

        { // vector length (more data)
            let value = vector[
                0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
                0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
                0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
                0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
                0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
                0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0
            ];

            let mut bytes = new(to_bytes(&value));
            assert!(value.length() == bytes.peel_vec_length());
        };

        { // full deserialization test (ordering)
            let info = Info { a: true, b: 100, c: 9999, d: 112333, k: vector[true, false, true, false], s: @0xAAAAAAAAAAA };
            let mut bytes = new(to_bytes(&info));

            assert!(info.a == bytes.peel_bool());
            assert!(info.b == bytes.peel_u8());
            assert!(info.c == bytes.peel_u64());
            assert!(info.d == bytes.peel_u128());

            let len = bytes.peel_vec_length();

            assert!(info.k.length() == len);

            let mut i = 0;
            while (i < info.k.length()) {
                assert!(info.k[i] == bytes.peel_bool());
                i = i + 1;
            };

            assert!(info.s == bytes.peel_address());
        };

        { // read vector of bytes directly
            let value = vector[
                vector[1,2,3,4,5],
                vector[1,2,3,4,5],
                vector[1,2,3,4,5]
            ];
            let mut bytes = new(to_bytes(&value));
            assert!(value == bytes.peel_vec_vec_u8());
        };

        { // read vector of bytes directly
            let value = vector[1,2,3,4,5];
            let mut bytes = new(to_bytes(&value));
            assert!(value == bytes.peel_vec_u8());
        };

        { // read vector of bytes directly
            let value = vector[1,2,3,4,5];
            let mut bytes = new(to_bytes(&value));
            assert!(value == bytes.peel_vec_u64());
        };

        { // read vector of bytes directly
            let value = vector[1,2,3,4,5];
            let mut bytes = new(to_bytes(&value));
            assert!(value == bytes.peel_vec_u128());
        };

        { // read vector of bytes directly
            let value = vector[true, false, true, false];
            let mut bytes = new(to_bytes(&value));
            assert!(value == bytes.peel_vec_bool());
        };

        { // read vector of address directly
            let value = vector[@0x0, @0x1, @0x2, @0x3];
            let mut bytes = new(to_bytes(&value));
            assert!(value == bytes.peel_vec_address());
        };
    }
}
