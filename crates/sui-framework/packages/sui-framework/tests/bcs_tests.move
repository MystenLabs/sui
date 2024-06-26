// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::bcs_tests {
    use sui::bcs::{Self, BCS, to_bytes, new};
    use std::unit_test::assert_eq;

    const U8_MAX: u8 = 0xFF;
    const U16_MAX: u16 = 0xFFFF;
    const U32_MAX: u32 = 0xFFFF_FFFF;
    const U64_MAX: u64 = 0xFFFF_FFFF_FFFF_FFFF;
    const U128_MAX: u128 = 0xFFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF;
    const U256_MAX: u256 =
        0xFFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF;

    public struct Info has copy, drop {
        a: bool,
        b: u8,
        c: u64,
        d: u128,
        k: vector<bool>,
        s: address,
    }

    #[test]
    #[expected_failure(abort_code = bcs::ELenOutOfRange)]
    fun test_uleb_len_fail() {
        let mut bytes = new(vector[0xff, 0xff, 0xff, 0xff, 0x80]);
        let _fail = bytes.peel_vec_length();
        abort 0
    }

    #[test]
    #[expected_failure(abort_code = bcs::ENotBool)]
    fun test_bool_fail() {
        let mut bytes = new(to_bytes(&10u8));
        let _fail = bytes.peel_bool();
    }

    macro fun cases<$T>(
        $cases: vector<$T>,
        $peel: |&mut BCS| -> $T,
    ) {
        let mut cases = $cases;
        while (!cases.is_empty()) {
            let case = cases.pop_back();
            let mut bytes = new(to_bytes(&case));
            assert_eq!($peel(&mut bytes), case);
            assert!(bytes.into_remainder_bytes().is_empty());
        };
    }

    macro fun num_cases<$T>($max: $T): vector<$T> {
        let max = $max;
        vector[
            0,
            1,
            max / 2,
            max - 1,
            max,
        ]
    }

    #[test]
    fun test_bool() {
        cases!(vector[true, false], |bytes| bytes.peel_bool());
    }

    #[test]
    fun test_u8() {
        cases!(num_cases!(U8_MAX), |bytes| bytes.peel_u8());
    }

    #[test]
    fun test_u16() {
        cases!(num_cases!(U16_MAX), |bytes| bytes.peel_u16());
    }

    #[test]
    fun test_u32() {
        cases!(num_cases!(U32_MAX), |bytes| bytes.peel_u32());
    }

    #[test]
    fun test_u64() {
        cases!(num_cases!(U64_MAX), |bytes| bytes.peel_u64());
    }

    #[test]
    fun test_u128() {
        cases!(num_cases!(U128_MAX), |bytes| bytes.peel_u128());
    }

    #[test]
    fun test_u256() {
        cases!(num_cases!(U256_MAX), |bytes| bytes.peel_u256());
    }

    #[test]
    fun test_address() {
        cases!(
            vector[
                @0x0,
                @0x1,
                @0xFFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF,
            ],
            |bytes| bytes.peel_address(),
        );
    }

    #[test]
    fun test_vec() {
        let bool_cases = vector[vector[], vector[true], vector[false, true, false]];
        cases!(bool_cases, |bytes| bytes.peel_vec_bool());
        cases!(bool_cases, |bytes| bytes.peel_vec!(|bytes| bytes.peel_bool()));

        let u8_cases = vector[vector[], vector[1], vector[0, 2, U8_MAX]];
        cases!(u8_cases, |bytes| bytes.peel_vec_u8());
        cases!(u8_cases, |bytes| bytes.peel_vec!(|bytes| bytes.peel_u8()));

        let u16_cases = vector[vector[], vector[1], vector[0, 2, U16_MAX]];
        cases!(u16_cases, |bytes| bytes.peel_vec_u16());
        cases!(u16_cases, |bytes| bytes.peel_vec!(|bytes| bytes.peel_u16()));

        let u32_cases = vector[vector[], vector[1], vector[0, 2, U32_MAX]];
        cases!(u32_cases, |bytes| bytes.peel_vec_u32());
        cases!(u32_cases, |bytes| bytes.peel_vec!(|bytes| bytes.peel_u32()));

        let u64_cases = vector[vector[], vector[1], vector[0, 2, U64_MAX]];
        cases!(u64_cases, |bytes| bytes.peel_vec_u64());
        cases!(u64_cases, |bytes| bytes.peel_vec!(|bytes| bytes.peel_u64()));

        let u128_cases = vector[vector[], vector[1], vector[0, 2, U128_MAX]];
        cases!(u128_cases, |bytes| bytes.peel_vec_u128());
        cases!(u128_cases, |bytes| bytes.peel_vec!(|bytes| bytes.peel_u128()));

        let u256_cases = vector[vector[], vector[1], vector[0, 2, U256_MAX]];
        cases!(u256_cases, |bytes| bytes.peel_vec_u256());
        cases!(u256_cases, |bytes| bytes.peel_vec!(|bytes| bytes.peel_u256()));

        let address_cases = vector[vector[], vector[@0x0], vector[@0x1, @0x2, @0x3]];
        cases!(address_cases, |bytes| bytes.peel_vec_address());
        cases!(address_cases, |bytes| bytes.peel_vec!(|bytes| bytes.peel_address()));
    }

    #[test]
    fun test_option() {
        let bool_cases = vector[option::none(), option::some(true), option::some(false)];
        cases!(bool_cases, |bytes| bytes.peel_option_bool());
        cases!(bool_cases, |bytes| bytes.peel_option!(|bytes| bytes.peel_bool()));

        let u8_cases = vector[option::none(), option::some(0), option::some(U8_MAX)];
        cases!(u8_cases, |bytes| bytes.peel_option_u8());
        cases!(u8_cases, |bytes| bytes.peel_option!(|bytes| bytes.peel_u8()));

        let u64_cases = vector[option::none(), option::some(0), option::some(U64_MAX)];
        cases!(u64_cases, |bytes| bytes.peel_option_u64());
        cases!(u64_cases, |bytes| bytes.peel_option!(|bytes| bytes.peel_u64()));

        let u128_cases = vector[option::none(), option::some(0), option::some(U128_MAX)];
        cases!(u128_cases, |bytes| bytes.peel_option_u128());
        cases!(u128_cases, |bytes| bytes.peel_option!(|bytes| bytes.peel_u128()));

        let u256_cases = vector[option::none(), option::some(0), option::some(U256_MAX)];
        cases!(u256_cases, |bytes| bytes.peel_option_u256());
        cases!(u256_cases, |bytes| bytes.peel_option!(|bytes| bytes.peel_u256()));

        let address_cases = vector[option::none(), option::some(@0x0), option::some(@0x1)];
        cases!(address_cases, |bytes| bytes.peel_option_address());
        cases!(address_cases, |bytes| bytes.peel_option!(|bytes| bytes.peel_address()));

        let opt_cases = vector[
            option::none(),
            option::some(option::none()),
            option::some(option::some(true))
        ];
        cases!(opt_cases, |bytes| bytes.peel_option!(|bytes| bytes.peel_option_bool()));
        cases!(
            opt_cases,
            |bytes| bytes.peel_option!(|bytes| bytes.peel_option!(|bytes| bytes.peel_bool())),
        );
    }

    #[test]
    fun test_complex() {
        let vec_vec_u8_cases = vector[vector[], vector[b"hello world"], vector[b"hello", b"world"]];
        cases!(vec_vec_u8_cases, |b| b.peel_vec_vec_u8());
        cases!(vec_vec_u8_cases, |b| b.peel_vec!(|b| b.peel_vec_u8()));
        cases!(vec_vec_u8_cases, |b| b.peel_vec!(|b| b.peel_vec!(|b| b.peel_u8())));

        let opt_vec_u8_cases = vector[
            option::none(),
            option::some(vector[]),
            option::some(vector[1]),
            option::some(vector[1, 2, U8_MAX]),
        ];
        cases!(opt_vec_u8_cases, |b| b.peel_option!(|b| b.peel_vec_u8()));
        cases!(opt_vec_u8_cases, |b| b.peel_option!(|b| b.peel_vec!(|b| b.peel_u8())));

        let vec_opt_u8_cases = vector[
            vector[option::none()],
            vector[option::some(1)],
            vector[option::some(1), option::none(), option::some(U8_MAX)],
        ];
        cases!(vec_opt_u8_cases, |b| b.peel_vec!(|b| b.peel_option_u8()));
        cases!(vec_opt_u8_cases, |b| b.peel_vec!(|b| b.peel_option!(|b| b.peel_u8())));
    }

    use fun peel_info as BCS.peel_info;
    fun peel_info(bytes: &mut BCS): Info {
        Info {
            a: bytes.peel_bool(),
            b: bytes.peel_u8(),
            c: bytes.peel_u64(),
            d: bytes.peel_u128(),
            k: bytes.peel_vec!(|bytes| bytes.peel_bool()),
            s: bytes.peel_address(),
        }
    }

    #[random_test]
    fun test_struct(a: bool, b: u8, c: u64, d: u128, k: vector<bool>, s: address) {
        let info = Info { a, b, c, d, k, s };
        let mut bytes = new(to_bytes(&info));

        assert_eq!(info.a, bytes.peel_bool());
        assert_eq!(info.b, bytes.peel_u8());
        assert_eq!(info.c, bytes.peel_u64());
        assert_eq!(info.d, bytes.peel_u128());

        let len = bytes.peel_vec_length();
        assert_eq!(info.k.length(), len);
        len.do!(|i| assert_eq!(info.k[i], bytes.peel_bool()));

        assert!(info.s == bytes.peel_address());

        let vec_cases = vector[vector[], vector[info], vector[info, info, info]];
        cases!(vec_cases, |bytes| bytes.peel_vec!(|bytes| bytes.peel_info()));

        let opt_cases = vector[option::none(), option::some(info)];
        cases!(opt_cases, |bytes| bytes.peel_option!(|bytes| bytes.peel_info()));
    }

}
