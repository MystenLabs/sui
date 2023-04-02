address 0x2 {
module M {
    #[test_only]
    use std::ascii;
    #[test_only]
    use std::debug::print;
    #[test_only]
    use std::debug::print_string;
    use std::string;
    #[test_only]
    use std::unit_test::create_signers_for_testing;
    #[test_only]
    use std::vector;

    struct Foo has drop {}
    struct Bar has drop { x: u128, y: Foo, z: bool }
    struct Box<T> has drop { x: T }

    struct GenericStruct<phantom T> has drop {
        val: u64,
    }

    struct TestInner has drop {
        val: u128,
        vec: vector<u128>,
        msgs: vector<vector<u8>>
    }

    struct TestStruct has drop {
        addr: address,
        number: u8,
        bytes: vector<u8>,
        name: string::String,
        vec: vector<TestInner>,
    }

    #[test]
    public fun test()  {
        let x = 42;
        print(&x);

        let v = vector::empty();
        vector::push_back(&mut v, 100);
        vector::push_back(&mut v, 200);
        vector::push_back(&mut v, 300);
        print(&v);

        let foo = Foo {};
        print(&foo);

        let bar = Bar { x: 404, y: Foo {}, z: true };
        print(&bar);

        let box = Box { x: Foo {} };
        print(&box);

        test_print_quoted_string();
        test_print_string();
        test_print_ascii_string();
        test_print_primitive_types();
        test_print_struct();
        test_print_vectors();
        test_print_vector_of_vectors();
        test_print_nested_struct();
        test_print_generic_struct();
    }

    #[test_only]
    fun test_print_string() {
        print_string(b"test_print_string");

        let str_bytes = b"Hello, sane Move debugging!";

        print(&str_bytes);

        let str = string::utf8(str_bytes);
        print<string::String>(&str);
    }

    #[test_only]
    fun test_print_ascii_string() {
        print_string(b"test_print_ascii_string");
        print(&ascii::string(b"Hello, sane Move debugging!"));
    }

    #[test_only]
    fun test_print_quoted_string() {
        print_string(b"test_print_quoted_string");

        let str_bytes = b"Can you say \"Hel\\lo\"?";

        let str = string::utf8(str_bytes);
        print<string::String>(&str);
    }

    #[test_only]
    fun test_print_primitive_types() {
        print_string(b"test_print_primitive_types");

        let u8 = 255u8;
        print(&u8);

        let u16 = 65535u16;
        print(&u16);

        let u32 = 4294967295u32;
        print(&u32);

        let u64 = 18446744073709551615u64;
        print(&u64);

        let u128 = 340282366920938463463374607431768211455u128;
        print(&u128);

        let u256 = 115792089237316195423570985008687907853269984665640564039457584007913129639935u256;
        print(&u256);

        let bool = false;
        print(&bool);

        let bool = true;
        print(&bool);

        let a = @0x1234c0ffee;
        print(&a);

        // print a signer
        let senders = create_signers_for_testing(1);
        let sender = vector::pop_back(&mut senders);
        print(&sender);
    }

    const MSG_1 : vector<u8> = b"abcdef";
    const MSG_2 : vector<u8> = b"123456";

    #[test_only]
    fun test_print_struct() {
        print_string(b"test_print_struct");

        let obj = TestInner {
            val: 100,
            vec: vector[200u128, 400u128],
            msgs: vector[MSG_1, MSG_2],
        };

        print(&obj);

        let obj = TestInner {
            val: 10,
            vec: vector[],
            msgs: vector[],
        };

        print(&obj);
    }

    #[test_only]
    fun test_print_vectors() {
        print_string(b"test_print_vectors");

        let v_u8 = x"ffabcdef";
        print(&v_u8);

        let v_u16 = vector[16u16, 17u16, 18u16, 19u16];
        print(&v_u16);

        let v_u32 = vector[32u32, 33u32, 34u32, 35u32];
        print(&v_u32);

        let v_u64 = vector[64u64, 65u64, 66u64, 67u64];
        print(&v_u64);

        let v_u128 = vector[128u128, 129u128, 130u128, 131u128];
        print(&v_u128);

        let v_u256 = vector[256u256, 257u256, 258u256, 259u256];
        print(&v_u256);

        let v_bool = vector[true, false];
        print(&v_bool);

        let v_addr = vector[@0x1234, @0x5678, @0xabcdef];
        print(&v_addr);

        let v_signer = create_signers_for_testing(4);
        print(&v_signer);

        let v = vector[
            TestInner {
                val: 4u128,
                vec: vector[127u128, 128u128],
                msgs: vector[x"00ff", x"abcd"],
            },
            TestInner {
                val: 8u128 ,
                vec: vector[128u128, 129u128],
                msgs: vector[x"0000"],
            }
        ];
        print(&v);
    }

    #[test_only]
    fun test_print_vector_of_vectors() {
        print_string(b"test_print_vector_of_vectors");

        let v_u8 = vector[x"ffab", x"cdef"];
        print(&v_u8);

        let v_u16 = vector[vector[16u16, 17u16], vector[18u16, 19u16]];
        print(&v_u16);

        let v_u32 = vector[vector[32u32, 33u32], vector[34u32, 35u32]];
        print(&v_u32);

        let v_u64 = vector[vector[64u64, 65u64], vector[66u64, 67u64]];
        print(&v_u64);

        let v_u128 = vector[vector[128u128, 129u128], vector[130u128, 131u128]];
        print(&v_u128);

        let v_u256 = vector[vector[256u256, 257u256], vector[258u256, 259u256]];
        print(&v_u256);

        let v_bool = vector[vector[true, false], vector[false, true]];
        print(&v_bool);

        let v_addr = vector[vector[@0x1234, @0x5678], vector[@0xabcdef, @0x9999]];
        print(&v_addr);

        let v_signer = vector[create_signers_for_testing(2), create_signers_for_testing(2)];
        print(&v_signer);

        let v = vector[
            vector[
                TestInner { val: 4u128, vec: vector[127u128, 128u128], msgs: vector[] },
                TestInner { val: 8u128 , vec: vector[128u128, 129u128], msgs: vector[] }
            ],
            vector[
                TestInner { val: 4u128, vec: vector[127u128, 128u128], msgs: vector[] },
                TestInner { val: 8u128 , vec: vector[128u128, 129u128], msgs: vector[] }
            ]
        ];
        print(&v);
    }

    #[test_only]
    fun test_print_nested_struct() {
        print_string(b"test_print_nested_struct");

        let obj = TestStruct {
            addr: @0x1,
            number: 255u8,
            bytes: x"c0ffee",
            name: string::utf8(b"He\"llo"),
            vec: vector[
                TestInner { val: 1, vec: vector[130u128, 131u128], msgs: vector[] },
                TestInner { val: 2, vec: vector[132u128, 133u128], msgs: vector[] }
            ],
        };

        print(&obj);
    }

    #[test_only]
    fun test_print_generic_struct() {
        print_string(b"test_print_generic_struct");

        let obj = GenericStruct<Foo> {
        val: 60u64,
        };

        print(&obj);
    }
}
}
