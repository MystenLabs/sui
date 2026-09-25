// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::execution::values::{prop::layout_and_value_strategy, *};
use move_core_types::{runtime_value::MoveValue, u256::U256};
use proptest::prelude::*;

/// Generates a matched pair of raw u64 values tagged with a byte-width discriminant.
/// Width is one of 1, 2, 4, 8, 16, 32 (the actual byte-width of the integer type).
fn tagged_u64_pair() -> impl Strategy<Value = (u8, u64, u64)> {
    prop_oneof![
        Just(1u8),
        Just(2u8),
        Just(4u8),
        Just(8u8),
        Just(16u8),
        Just(32u8),
    ]
    .prop_flat_map(|w| (Just(w), any::<u64>(), any::<u64>()))
}

/// Constructs an IntegerValue from a byte-width tag and raw u64 (truncated).
fn make_int(width: u8, raw: u64) -> IntegerValue {
    match width {
        1 => IntegerValue::U8(raw as u8),
        2 => IntegerValue::U16(raw as u16),
        4 => IntegerValue::U32(raw as u32),
        8 => IntegerValue::U64(raw),
        16 => IntegerValue::U128(raw as u128),
        32 => IntegerValue::U256(move_core_types::u256::U256::from(raw)),
        _ => unreachable!("invalid byte-width: {width}"),
    }
}

/// Constructs the zero IntegerValue for a given byte-width tag.
fn make_zero(width: u8) -> IntegerValue {
    make_int(width, 0)
}

/// Extracts the numeric value from an IntegerValue as u128.
/// Safe for all widths since inputs are generated from u64 (results fit in u128).
fn int_to_u128(v: IntegerValue) -> u128 {
    match v {
        IntegerValue::U8(x) => x as u128,
        IntegerValue::U16(x) => x as u128,
        IntegerValue::U32(x) => x as u128,
        IntegerValue::U64(x) => x as u128,
        IntegerValue::U128(x) => x,
        IntegerValue::U256(x) => x.try_into().unwrap(),
    }
}

/// Performs a native Rust checked arithmetic op on truncated values, returning
/// the result as a u128 (None if overflow/div-by-zero). This serves as the
/// reference implementation to compare VM results against.
fn native_checked_op(width: u8, ra: u64, rb: u64, op: &str) -> Option<u128> {
    match width {
        1 => {
            let (a, b) = (ra as u8, rb as u8);
            match op {
                "add" => a.checked_add(b).map(|v| v as u128),
                "sub" => a.checked_sub(b).map(|v| v as u128),
                "mul" => a.checked_mul(b).map(|v| v as u128),
                "div" => a.checked_div(b).map(|v| v as u128),
                "rem" => a.checked_rem(b).map(|v| v as u128),
                _ => unreachable!(),
            }
        }
        2 => {
            let (a, b) = (ra as u16, rb as u16);
            match op {
                "add" => a.checked_add(b).map(|v| v as u128),
                "sub" => a.checked_sub(b).map(|v| v as u128),
                "mul" => a.checked_mul(b).map(|v| v as u128),
                "div" => a.checked_div(b).map(|v| v as u128),
                "rem" => a.checked_rem(b).map(|v| v as u128),
                _ => unreachable!(),
            }
        }
        4 => {
            let (a, b) = (ra as u32, rb as u32);
            match op {
                "add" => a.checked_add(b).map(|v| v as u128),
                "sub" => a.checked_sub(b).map(|v| v as u128),
                "mul" => a.checked_mul(b).map(|v| v as u128),
                "div" => a.checked_div(b).map(|v| v as u128),
                "rem" => a.checked_rem(b).map(|v| v as u128),
                _ => unreachable!(),
            }
        }
        8 => {
            let (a, b) = (ra, rb);
            match op {
                "add" => a.checked_add(b).map(|v| v as u128),
                "sub" => a.checked_sub(b).map(|v| v as u128),
                "mul" => a.checked_mul(b).map(|v| v as u128),
                "div" => a.checked_div(b).map(|v| v as u128),
                "rem" => a.checked_rem(b).map(|v| v as u128),
                _ => unreachable!(),
            }
        }
        16 => {
            let (a, b) = (ra as u128, rb as u128);
            match op {
                "add" => a.checked_add(b),
                "sub" => a.checked_sub(b),
                "mul" => a.checked_mul(b),
                "div" => a.checked_div(b),
                "rem" => a.checked_rem(b),
                _ => unreachable!(),
            }
        }
        32 => {
            let a = U256::from(ra);
            let b = U256::from(rb);
            match op {
                "add" => a.checked_add(b).map(|r| r.try_into().unwrap()),
                "sub" => a.checked_sub(b).map(|r| r.try_into().unwrap()),
                "mul" => a.checked_mul(b).map(|r| r.try_into().unwrap()),
                "div" => a.checked_div(b).map(|r| r.try_into().unwrap()),
                "rem" => a.checked_rem(b).map(|r| r.try_into().unwrap()),
                _ => unreachable!(),
            }
        }
        _ => unreachable!(),
    }
}

/// Performs a native Rust comparison on truncated values.
fn native_cmp(width: u8, ra: u64, rb: u64, op: &str) -> bool {
    match width {
        1 => {
            let (a, b) = (ra as u8, rb as u8);
            match op {
                "lt" => a < b,
                "le" => a <= b,
                "gt" => a > b,
                "ge" => a >= b,
                _ => unreachable!(),
            }
        }
        2 => {
            let (a, b) = (ra as u16, rb as u16);
            match op {
                "lt" => a < b,
                "le" => a <= b,
                "gt" => a > b,
                "ge" => a >= b,
                _ => unreachable!(),
            }
        }
        4 => {
            let (a, b) = (ra as u32, rb as u32);
            match op {
                "lt" => a < b,
                "le" => a <= b,
                "gt" => a > b,
                "ge" => a >= b,
                _ => unreachable!(),
            }
        }
        8 => {
            let (a, b) = (ra, rb);
            match op {
                "lt" => a < b,
                "le" => a <= b,
                "gt" => a > b,
                "ge" => a >= b,
                _ => unreachable!(),
            }
        }
        16 => {
            let (a, b) = (ra as u128, rb as u128);
            match op {
                "lt" => a < b,
                "le" => a <= b,
                "gt" => a > b,
                "ge" => a >= b,
                _ => unreachable!(),
            }
        }
        32 => {
            let (a, b) = (U256::from(ra), U256::from(rb));
            match op {
                "lt" => a < b,
                "le" => a <= b,
                "gt" => a > b,
                "ge" => a >= b,
                _ => unreachable!(),
            }
        }
        _ => unreachable!(),
    }
}

/// Performs a native Rust bitwise op on truncated values, returns as u128.
fn native_bitwise(width: u8, ra: u64, rb: u64, op: &str) -> u128 {
    match width {
        1 => {
            let (a, b) = (ra as u8, rb as u8);
            (match op {
                "or" => a | b,
                "and" => a & b,
                "xor" => a ^ b,
                _ => unreachable!(),
            }) as u128
        }
        2 => {
            let (a, b) = (ra as u16, rb as u16);
            (match op {
                "or" => a | b,
                "and" => a & b,
                "xor" => a ^ b,
                _ => unreachable!(),
            }) as u128
        }
        4 => {
            let (a, b) = (ra as u32, rb as u32);
            (match op {
                "or" => a | b,
                "and" => a & b,
                "xor" => a ^ b,
                _ => unreachable!(),
            }) as u128
        }
        8 => {
            let (a, b) = (ra, rb);
            (match op {
                "or" => a | b,
                "and" => a & b,
                "xor" => a ^ b,
                _ => unreachable!(),
            }) as u128
        }
        16 => {
            let (a, b) = (ra as u128, rb as u128);
            match op {
                "or" => a | b,
                "and" => a & b,
                "xor" => a ^ b,
                _ => unreachable!(),
            }
        }
        32 => {
            let (a, b) = (U256::from(ra), U256::from(rb));
            (match op {
                "or" => a | b,
                "and" => a & b,
                "xor" => a ^ b,
                _ => unreachable!(),
            })
            .try_into()
            .unwrap()
        }
        _ => unreachable!(),
    }
}

proptest! {
    /// serialize -> deserialize round-trip for both VM values and MoveValues.
    #[test]
    fn serializer_round_trip((layout, value) in layout_and_value_strategy()) {
        let blob = value.typed_serialize(&layout).expect("must serialize");

        let value_deserialized = Value::simple_deserialize(&blob, &layout).expect("must deserialize");
        assert!(value.equals(&value_deserialized).unwrap());

        let move_value = value.as_move_value(&layout).expect("must convert to MoveValue");

        let blob2 = move_value.simple_serialize().expect("must serialize");
        assert_eq!(blob, blob2);

        let move_value_deserialized = MoveValue::simple_deserialize(&blob2, &layout).expect("must deserialize.");
        assert_eq!(move_value, move_value_deserialized);
    }

    /// copy_value always produces an equal value.
    #[test]
    fn copy_value_preserves_equality((_layout, value) in layout_and_value_strategy()) {
        let copy = value.copy_value();
        assert!(value.equals(&copy).unwrap());
    }

    /// Every value equals itself.
    #[test]
    fn equals_is_reflexive((_, value) in layout_and_value_strategy()) {
        assert!(value.equals(&value).unwrap());
    }

    /// a.equals(b) == b.equals(a) for any pair of equal values.
    #[test]
    fn equals_is_symmetric((_layout, a) in layout_and_value_strategy()) {
        let b = a.copy_value();
        let ab = a.equals(&b).unwrap();
        let ba = b.equals(&a).unwrap();
        assert_eq!(ab, ba);
    }

    /// Vector::pack -> unpack round-trip preserves all elements.
    #[test]
    fn vector_pack_unpack_round_trip(values in
        proptest::collection::vec(any::<u64>(), 0..10)
    ) {
        let original_values: Vec<Value> = values.iter().map(|&v| Value::u64(v)).collect();
        let packed = Vector::pack(
            VectorSpecialization::U64,
            original_values.iter().map(Value::copy_value),
        ).unwrap();
        let vec: Vector = VMValueCast::cast(packed).unwrap();
        let unpacked = vec.unpack(
            &crate::jit::execution::ast::Type::U64,
            values.len() as u64,
        ).unwrap();
        assert_eq!(unpacked.len(), original_values.len());
        for (a, b) in original_values.iter().zip(unpacked.iter()) {
            assert!(a.equals(b).unwrap());
        }
    }

    // -----------------------------------------------------------------------
    // Integer Arithmetic — VM matches native Rust
    // -----------------------------------------------------------------------

    /// Arithmetic between mismatched widths always errors (all width pairs).
    #[test]
    fn integer_type_mismatch_always_errors((w1, ra, _) in tagged_u64_pair(), w2 in prop_oneof![Just(1u8), Just(2u8), Just(4u8), Just(8u8), Just(16u8), Just(32u8)]) {
        if w1 == w2 { return Ok(()); }
        assert!(make_int(w1, ra).add_checked(make_int(w2, ra)).is_err());
    }

    // -----------------------------------------------------------------------
    // Arithmetic Correctness — VM results match native Rust
    // -----------------------------------------------------------------------

    /// VM add matches native Rust checked_add (all widths).
    #[test]
    fn integer_add_matches_native((w, ra, rb) in tagged_u64_pair()) {
        let vm_result = make_int(w, ra).add_checked(make_int(w, rb));
        match native_checked_op(w, ra, rb, "add") {
            Some(expected) => assert_eq!(int_to_u128(vm_result.unwrap()), expected),
            None => assert!(vm_result.is_err()),
        }
    }

    /// VM sub matches native Rust checked_sub (all widths).
    #[test]
    fn integer_sub_matches_native((w, ra, rb) in tagged_u64_pair()) {
        let vm_result = make_int(w, ra).sub_checked(make_int(w, rb));
        match native_checked_op(w, ra, rb, "sub") {
            Some(expected) => assert_eq!(int_to_u128(vm_result.unwrap()), expected),
            None => assert!(vm_result.is_err()),
        }
    }

    /// VM mul matches native Rust checked_mul (all widths).
    #[test]
    fn integer_mul_matches_native((w, ra, rb) in tagged_u64_pair()) {
        let vm_result = make_int(w, ra).mul_checked(make_int(w, rb));
        match native_checked_op(w, ra, rb, "mul") {
            Some(expected) => assert_eq!(int_to_u128(vm_result.unwrap()), expected),
            None => assert!(vm_result.is_err()),
        }
    }

    /// VM div matches native Rust checked_div (all widths).
    #[test]
    fn integer_div_matches_native((w, ra, rb) in tagged_u64_pair()) {
        let vm_result = make_int(w, ra).div_checked(make_int(w, rb));
        match native_checked_op(w, ra, rb, "div") {
            Some(expected) => assert_eq!(int_to_u128(vm_result.unwrap()), expected),
            None => assert!(vm_result.is_err()),
        }
    }

    /// VM rem matches native Rust checked_rem (all widths).
    #[test]
    fn integer_rem_matches_native((w, ra, rb) in tagged_u64_pair()) {
        let vm_result = make_int(w, ra).rem_checked(make_int(w, rb));
        match native_checked_op(w, ra, rb, "rem") {
            Some(expected) => assert_eq!(int_to_u128(vm_result.unwrap()), expected),
            None => assert!(vm_result.is_err()),
        }
    }

    // -----------------------------------------------------------------------
    // Bitwise Correctness — VM results match native Rust
    // -----------------------------------------------------------------------

    /// VM bit_or matches native Rust | (all widths).
    #[test]
    fn integer_bit_or_matches_native((w, ra, rb) in tagged_u64_pair()) {
        let result = make_int(w, ra).bit_or(make_int(w, rb)).unwrap();
        assert_eq!(int_to_u128(result), native_bitwise(w, ra, rb, "or"));
    }

    /// VM bit_and matches native Rust & (all widths).
    #[test]
    fn integer_bit_and_matches_native((w, ra, rb) in tagged_u64_pair()) {
        let result = make_int(w, ra).bit_and(make_int(w, rb)).unwrap();
        assert_eq!(int_to_u128(result), native_bitwise(w, ra, rb, "and"));
    }

    /// VM bit_xor matches native Rust ^ (all widths).
    #[test]
    fn integer_bit_xor_matches_native((w, ra, rb) in tagged_u64_pair()) {
        let result = make_int(w, ra).bit_xor(make_int(w, rb)).unwrap();
        assert_eq!(int_to_u128(result), native_bitwise(w, ra, rb, "xor"));
    }

    // -----------------------------------------------------------------------
    // Shift Correctness — VM results match native Rust
    // -----------------------------------------------------------------------

    /// VM shl matches native Rust << for valid shift amounts (all widths).
    #[test]
    fn integer_shl_matches_native((w, ra, _) in tagged_u64_pair(), n in 0..8u8) {
        // n < 8 is always a valid shift for all widths (smallest is u8 = 8 bits).
        let vm_result = make_int(w, ra).shl_checked(n).unwrap();
        let expected: u128 = match w {
            1 => ((ra as u8) << n) as u128,
            2 => ((ra as u16) << n) as u128,
            4 => ((ra as u32) << n) as u128,
            8 => ((ra) << n) as u128,
            16 => (ra as u128) << n,
            32 => (U256::from(ra) << (n as u32)).try_into().unwrap(),
            _ => unreachable!(),
        };
        assert_eq!(int_to_u128(vm_result), expected);
    }

    /// VM shr matches native Rust >> for valid shift amounts (all widths).
    #[test]
    fn integer_shr_matches_native((w, ra, _) in tagged_u64_pair(), n in 0..8u8) {
        let vm_result = make_int(w, ra).shr_checked(n).unwrap();
        let expected: u128 = match w {
            1 => ((ra as u8) >> n) as u128,
            2 => ((ra as u16) >> n) as u128,
            4 => ((ra as u32) >> n) as u128,
            8 => (ra >> n) as u128,
            16 => (ra as u128) >> n,
            32 => (U256::from(ra) >> n).try_into().unwrap(),
            _ => unreachable!(),
        };
        assert_eq!(int_to_u128(vm_result), expected);
    }

    // -----------------------------------------------------------------------
    // Comparison Correctness — VM results match native Rust
    // -----------------------------------------------------------------------

    /// VM lt matches native Rust < (all widths).
    #[test]
    fn integer_lt_matches_native((w, ra, rb) in tagged_u64_pair()) {
        assert_eq!(make_int(w, ra).lt(make_int(w, rb)).unwrap(), native_cmp(w, ra, rb, "lt"));
    }

    /// VM le matches native Rust <= (all widths).
    #[test]
    fn integer_le_matches_native((w, ra, rb) in tagged_u64_pair()) {
        assert_eq!(make_int(w, ra).le(make_int(w, rb)).unwrap(), native_cmp(w, ra, rb, "le"));
    }

    /// VM gt matches native Rust > (all widths).
    #[test]
    fn integer_gt_matches_native((w, ra, rb) in tagged_u64_pair()) {
        assert_eq!(make_int(w, ra).gt(make_int(w, rb)).unwrap(), native_cmp(w, ra, rb, "gt"));
    }

    /// VM ge matches native Rust >= (all widths).
    #[test]
    fn integer_ge_matches_native((w, ra, rb) in tagged_u64_pair()) {
        assert_eq!(make_int(w, ra).ge(make_int(w, rb)).unwrap(), native_cmp(w, ra, rb, "ge"));
    }

    // -----------------------------------------------------------------------
    // Casting Properties — Widening (every source type to every wider target)
    // -----------------------------------------------------------------------

    /// u8 widens to all larger types, preserving value.
    #[test]
    fn widening_u8_to_all(x in any::<u8>()) {
        assert_eq!(IntegerValue::U8(x).cast_u16().unwrap(), x as u16);
        assert_eq!(IntegerValue::U8(x).cast_u32().unwrap(), x as u32);
        assert_eq!(IntegerValue::U8(x).cast_u64().unwrap(), x as u64);
        assert_eq!(IntegerValue::U8(x).cast_u128().unwrap(), x as u128);
        assert_eq!(IntegerValue::U8(x).cast_u256().unwrap(), U256::from(x as u64));
    }

    /// u16 widens to u32/u64/u128/u256.
    #[test]
    fn widening_u16_to_all(x in any::<u16>()) {
        assert_eq!(IntegerValue::U16(x).cast_u32().unwrap(), x as u32);
        assert_eq!(IntegerValue::U16(x).cast_u64().unwrap(), x as u64);
        assert_eq!(IntegerValue::U16(x).cast_u128().unwrap(), x as u128);
        assert_eq!(IntegerValue::U16(x).cast_u256().unwrap(), U256::from(x as u64));
    }

    /// u32 widens to u64/u128/u256.
    #[test]
    fn widening_u32_to_all(x in any::<u32>()) {
        assert_eq!(IntegerValue::U32(x).cast_u64().unwrap(), x as u64);
        assert_eq!(IntegerValue::U32(x).cast_u128().unwrap(), x as u128);
        assert_eq!(IntegerValue::U32(x).cast_u256().unwrap(), U256::from(x as u64));
    }

    /// u64 widens to u128/u256.
    #[test]
    fn widening_u64_to_all(x in any::<u64>()) {
        assert_eq!(IntegerValue::U64(x).cast_u128().unwrap(), x as u128);
        assert_eq!(IntegerValue::U64(x).cast_u256().unwrap(), U256::from(x));
    }

    /// u128 widens to u256.
    #[test]
    fn widening_u128_to_u256(x in any::<u128>()) {
        assert_eq!(IntegerValue::U128(x).cast_u256().unwrap(), U256::from(x));
    }

    // -----------------------------------------------------------------------
    // Casting Properties — Narrowing (fails iff value exceeds target range)
    // -----------------------------------------------------------------------

    /// u16 -> u8: succeeds iff x <= u8::MAX.
    #[test]
    fn narrowing_u16_to_u8(x in any::<u16>()) {
        let result = IntegerValue::U16(x).cast_u8();
        if x <= u8::MAX as u16 { assert_eq!(result.unwrap(), x as u8); }
        else { assert!(result.is_err()); }
    }

    /// u32 -> u8: succeeds iff x <= u8::MAX.
    #[test]
    fn narrowing_u32_to_u8(x in any::<u32>()) {
        let result = IntegerValue::U32(x).cast_u8();
        if x <= u8::MAX as u32 { assert_eq!(result.unwrap(), x as u8); }
        else { assert!(result.is_err()); }
    }

    /// u32 -> u16: succeeds iff x <= u16::MAX.
    #[test]
    fn narrowing_u32_to_u16(x in any::<u32>()) {
        let result = IntegerValue::U32(x).cast_u16();
        if x <= u16::MAX as u32 { assert_eq!(result.unwrap(), x as u16); }
        else { assert!(result.is_err()); }
    }

    /// u64 -> u8: succeeds iff x <= u8::MAX.
    #[test]
    fn narrowing_u64_to_u8(x in any::<u64>()) {
        let result = IntegerValue::U64(x).cast_u8();
        if x <= u8::MAX as u64 { assert_eq!(result.unwrap(), x as u8); }
        else { assert!(result.is_err()); }
    }

    /// u64 -> u16: succeeds iff x <= u16::MAX.
    #[test]
    fn narrowing_u64_to_u16(x in any::<u64>()) {
        let result = IntegerValue::U64(x).cast_u16();
        if x <= u16::MAX as u64 { assert_eq!(result.unwrap(), x as u16); }
        else { assert!(result.is_err()); }
    }

    /// u64 -> u32: succeeds iff x <= u32::MAX.
    #[test]
    fn narrowing_u64_to_u32(x in any::<u64>()) {
        let result = IntegerValue::U64(x).cast_u32();
        if x <= u32::MAX as u64 { assert_eq!(result.unwrap(), x as u32); }
        else { assert!(result.is_err()); }
    }

    /// u128 -> u8: succeeds iff x <= u8::MAX.
    #[test]
    fn narrowing_u128_to_u8(x in any::<u128>()) {
        let result = IntegerValue::U128(x).cast_u8();
        if x <= u8::MAX as u128 { assert_eq!(result.unwrap(), x as u8); }
        else { assert!(result.is_err()); }
    }

    /// u128 -> u16: succeeds iff x <= u16::MAX.
    #[test]
    fn narrowing_u128_to_u16(x in any::<u128>()) {
        let result = IntegerValue::U128(x).cast_u16();
        if x <= u16::MAX as u128 { assert_eq!(result.unwrap(), x as u16); }
        else { assert!(result.is_err()); }
    }

    /// u128 -> u32: succeeds iff x <= u32::MAX.
    #[test]
    fn narrowing_u128_to_u32(x in any::<u128>()) {
        let result = IntegerValue::U128(x).cast_u32();
        if x <= u32::MAX as u128 { assert_eq!(result.unwrap(), x as u32); }
        else { assert!(result.is_err()); }
    }

    /// u128 -> u64: succeeds iff x <= u64::MAX.
    #[test]
    fn narrowing_u128_to_u64(x in any::<u128>()) {
        let result = IntegerValue::U128(x).cast_u64();
        if x <= u64::MAX as u128 { assert_eq!(result.unwrap(), x as u64); }
        else { assert!(result.is_err()); }
    }

    /// u256 -> u8: succeeds iff x <= u8::MAX.
    #[test]
    fn narrowing_u256_to_u8(raw in any::<u128>()) {
        let x = U256::from(raw);
        let result = IntegerValue::U256(x).cast_u8();
        if raw <= u8::MAX as u128 { assert_eq!(result.unwrap(), raw as u8); }
        else { assert!(result.is_err()); }
    }

    /// u256 -> u16: succeeds iff x <= u16::MAX.
    #[test]
    fn narrowing_u256_to_u16(raw in any::<u128>()) {
        let x = U256::from(raw);
        let result = IntegerValue::U256(x).cast_u16();
        if raw <= u16::MAX as u128 { assert_eq!(result.unwrap(), raw as u16); }
        else { assert!(result.is_err()); }
    }

    /// u256 -> u32: succeeds iff x <= u32::MAX.
    #[test]
    fn narrowing_u256_to_u32(raw in any::<u128>()) {
        let x = U256::from(raw);
        let result = IntegerValue::U256(x).cast_u32();
        if raw <= u32::MAX as u128 { assert_eq!(result.unwrap(), raw as u32); }
        else { assert!(result.is_err()); }
    }

    /// u256 -> u64: succeeds iff x <= u64::MAX.
    #[test]
    fn narrowing_u256_to_u64(raw in any::<u128>()) {
        let x = U256::from(raw);
        let result = IntegerValue::U256(x).cast_u64();
        if raw <= u64::MAX as u128 { assert_eq!(result.unwrap(), raw as u64); }
        else { assert!(result.is_err()); }
    }

    /// u256 -> u128: succeeds iff x <= u128::MAX.
    #[test]
    fn narrowing_u256_to_u128(raw in any::<u128>()) {
        // Values constructed from u128 always fit, so also test above u128::MAX.
        let x = U256::from(raw);
        assert_eq!(IntegerValue::U256(x).cast_u128().unwrap(), raw);
    }

    /// u256 values above u128::MAX fail to narrow to u128.
    #[test]
    fn narrowing_u256_above_u128_max_fails(lo in any::<u128>(), hi in 1..=u128::MAX) {
        // Construct a value > u128::MAX by setting high bits.
        let x = U256::from(lo) + (U256::from(hi) << 128u32);
        assert!(IntegerValue::U256(x).cast_u128().is_err());
    }

    // -----------------------------------------------------------------------
    // Casting identity — casting to own width is always the identity.
    // -----------------------------------------------------------------------

    /// Casting to the same width always succeeds and is the identity.
    #[test]
    fn cast_same_width_is_identity((w, ra, _) in tagged_u64_pair()) {
        let v = make_int(w, ra);
        match w {
            1 => { let x: u8 = VMValueCast::cast(v.into_value()).unwrap();
                   assert_eq!(x, ra as u8); }
            2 => { let x: u16 = VMValueCast::cast(v.into_value()).unwrap();
                   assert_eq!(x, ra as u16); }
            4 => { let x: u32 = VMValueCast::cast(v.into_value()).unwrap();
                   assert_eq!(x, ra as u32); }
            8 => { let x: u64 = VMValueCast::cast(v.into_value()).unwrap();
                   assert_eq!(x, ra); }
            16 => { let x: u128 = VMValueCast::cast(v.into_value()).unwrap();
                    assert_eq!(x, ra as u128); }
            32 => { assert!(v.cast_u256().is_ok()); }
            _ => unreachable!(),
        }
    }
}
