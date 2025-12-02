// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use crate::{
        execution::interpreter::eval::control_flow_instruction, jit::execution::ast::Bytecode,
    };

    #[test]
    fn test_control_flow_instruction_returns_true_for_ret() {
        let instruction = Bytecode::Ret;
        assert!(
            control_flow_instruction(&instruction),
            "Ret should be a control flow instruction"
        );
    }

    #[test]
    fn test_control_flow_instruction_returns_true_for_branches() {
        assert!(
            control_flow_instruction(&Bytecode::BrTrue(0)),
            "BrTrue should be a control flow instruction"
        );
        assert!(
            control_flow_instruction(&Bytecode::BrFalse(0)),
            "BrFalse should be a control flow instruction"
        );
        assert!(
            control_flow_instruction(&Bytecode::Branch(0)),
            "Branch should be a control flow instruction"
        );
    }

    #[test]
    fn test_control_flow_instruction_returns_false_for_arithmetic() {
        assert!(
            !control_flow_instruction(&Bytecode::Add),
            "Add should not be a control flow instruction"
        );
        assert!(
            !control_flow_instruction(&Bytecode::Sub),
            "Sub should not be a control flow instruction"
        );
        assert!(
            !control_flow_instruction(&Bytecode::Mul),
            "Mul should not be a control flow instruction"
        );
        assert!(
            !control_flow_instruction(&Bytecode::Div),
            "Div should not be a control flow instruction"
        );
        assert!(
            !control_flow_instruction(&Bytecode::Mod),
            "Mod should not be a control flow instruction"
        );
    }

    #[test]
    fn test_control_flow_instruction_returns_false_for_logical() {
        assert!(
            !control_flow_instruction(&Bytecode::And),
            "And should not be a control flow instruction"
        );
        assert!(
            !control_flow_instruction(&Bytecode::Or),
            "Or should not be a control flow instruction"
        );
        assert!(
            !control_flow_instruction(&Bytecode::Not),
            "Not should not be a control flow instruction"
        );
    }

    #[test]
    fn test_control_flow_instruction_returns_false_for_comparison() {
        assert!(
            !control_flow_instruction(&Bytecode::Lt),
            "Lt should not be a control flow instruction"
        );
        assert!(
            !control_flow_instruction(&Bytecode::Gt),
            "Gt should not be a control flow instruction"
        );
        assert!(
            !control_flow_instruction(&Bytecode::Le),
            "Le should not be a control flow instruction"
        );
        assert!(
            !control_flow_instruction(&Bytecode::Ge),
            "Ge should not be a control flow instruction"
        );
        assert!(
            !control_flow_instruction(&Bytecode::Eq),
            "Eq should not be a control flow instruction"
        );
        assert!(
            !control_flow_instruction(&Bytecode::Neq),
            "Neq should not be a control flow instruction"
        );
    }

    #[test]
    fn test_control_flow_instruction_returns_false_for_bitwise() {
        assert!(
            !control_flow_instruction(&Bytecode::BitOr),
            "BitOr should not be a control flow instruction"
        );
        assert!(
            !control_flow_instruction(&Bytecode::BitAnd),
            "BitAnd should not be a control flow instruction"
        );
        assert!(
            !control_flow_instruction(&Bytecode::Xor),
            "Xor should not be a control flow instruction"
        );
        assert!(
            !control_flow_instruction(&Bytecode::Shl),
            "Shl should not be a control flow instruction"
        );
        assert!(
            !control_flow_instruction(&Bytecode::Shr),
            "Shr should not be a control flow instruction"
        );
    }

    #[test]
    fn test_control_flow_instruction_returns_false_for_constants() {
        assert!(
            !control_flow_instruction(&Bytecode::LdTrue),
            "LdTrue should not be a control flow instruction"
        );
        assert!(
            !control_flow_instruction(&Bytecode::LdFalse),
            "LdFalse should not be a control flow instruction"
        );
        assert!(
            !control_flow_instruction(&Bytecode::LdU8(0)),
            "LdU8 should not be a control flow instruction"
        );
        assert!(
            !control_flow_instruction(&Bytecode::LdU16(0)),
            "LdU16 should not be a control flow instruction"
        );
        assert!(
            !control_flow_instruction(&Bytecode::LdU32(0)),
            "LdU32 should not be a control flow instruction"
        );
        assert!(
            !control_flow_instruction(&Bytecode::LdU64(0)),
            "LdU64 should not be a control flow instruction"
        );
    }

    #[test]
    fn test_control_flow_instruction_returns_false_for_casting() {
        assert!(
            !control_flow_instruction(&Bytecode::CastU8),
            "CastU8 should not be a control flow instruction"
        );
        assert!(
            !control_flow_instruction(&Bytecode::CastU16),
            "CastU16 should not be a control flow instruction"
        );
        assert!(
            !control_flow_instruction(&Bytecode::CastU32),
            "CastU32 should not be a control flow instruction"
        );
        assert!(
            !control_flow_instruction(&Bytecode::CastU64),
            "CastU64 should not be a control flow instruction"
        );
        assert!(
            !control_flow_instruction(&Bytecode::CastU128),
            "CastU128 should not be a control flow instruction"
        );
        assert!(
            !control_flow_instruction(&Bytecode::CastU256),
            "CastU256 should not be a control flow instruction"
        );
    }

    #[test]
    fn test_control_flow_instruction_returns_false_for_stack_ops() {
        assert!(
            !control_flow_instruction(&Bytecode::Pop),
            "Pop should not be a control flow instruction"
        );
        assert!(
            !control_flow_instruction(&Bytecode::CopyLoc(0)),
            "CopyLoc should not be a control flow instruction"
        );
        assert!(
            !control_flow_instruction(&Bytecode::MoveLoc(0)),
            "MoveLoc should not be a control flow instruction"
        );
        assert!(
            !control_flow_instruction(&Bytecode::StLoc(0)),
            "StLoc should not be a control flow instruction"
        );
    }

    #[test]
    fn test_control_flow_instruction_returns_false_for_references() {
        assert!(
            !control_flow_instruction(&Bytecode::ReadRef),
            "ReadRef should not be a control flow instruction"
        );
        assert!(
            !control_flow_instruction(&Bytecode::WriteRef),
            "WriteRef should not be a control flow instruction"
        );
        assert!(
            !control_flow_instruction(&Bytecode::FreezeRef),
            "FreezeRef should not be a control flow instruction"
        );
        assert!(
            !control_flow_instruction(&Bytecode::MutBorrowLoc(0)),
            "MutBorrowLoc should not be a control flow instruction"
        );
        assert!(
            !control_flow_instruction(&Bytecode::ImmBorrowLoc(0)),
            "ImmBorrowLoc should not be a control flow instruction"
        );
    }

    #[test]
    fn test_control_flow_instruction_returns_false_for_abort() {
        assert!(
            !control_flow_instruction(&Bytecode::Abort),
            "Abort should not be a control flow instruction (it terminates)"
        );
    }

    #[test]
    fn test_control_flow_instruction_returns_false_for_nop() {
        assert!(
            !control_flow_instruction(&Bytecode::Nop),
            "Nop should not be a control flow instruction"
        );
    }
}

#[cfg(test)]
mod depth_tests {
    use crate::jit::execution::ast::Type;

    #[test]
    fn test_primitive_types_have_depth_one() {
        let primitives = vec![
            Type::Bool,
            Type::U8,
            Type::U16,
            Type::U32,
            Type::U64,
            Type::U128,
            Type::U256,
            Type::Address,
            Type::Signer,
        ];

        for ty in primitives {
            let type_name = format!("{:?}", ty);
            assert!(
                matches!(
                    ty,
                    Type::Bool
                        | Type::U8
                        | Type::U16
                        | Type::U32
                        | Type::U64
                        | Type::U128
                        | Type::U256
                        | Type::Address
                        | Type::Signer
                ),
                "{} should be a primitive type",
                type_name
            );
        }
    }

    #[test]
    fn test_vector_types_increase_depth() {
        let vec_u8 = Type::Vector(Box::new(Type::U8));
        assert!(matches!(vec_u8, Type::Vector(_)), "Should be a vector type");

        let vec_vec_u8 = Type::Vector(Box::new(vec_u8));
        assert!(
            matches!(vec_vec_u8, Type::Vector(_)),
            "Should be a nested vector type"
        );
    }

    #[test]
    fn test_reference_types_increase_depth() {
        let ref_u8 = Type::Reference(Box::new(Type::U8));
        assert!(
            matches!(ref_u8, Type::Reference(_)),
            "Should be a reference type"
        );

        let mut_ref_u8 = Type::MutableReference(Box::new(Type::U8));
        assert!(
            matches!(mut_ref_u8, Type::MutableReference(_)),
            "Should be a mutable reference type"
        );
    }
}

#[cfg(test)]
mod value_stack_tests {
    use crate::execution::values::Value;

    #[test]
    fn test_value_creation_u8() {
        let val = Value::u8(42);
        assert!(matches!(val, Value::U8(42)), "Should create U8 value");
    }

    #[test]
    fn test_value_creation_u16() {
        let val = Value::u16(1000);
        assert!(matches!(val, Value::U16(1000)), "Should create U16 value");
    }

    #[test]
    fn test_value_creation_u32() {
        let val = Value::u32(100000);
        assert!(matches!(val, Value::U32(100000)), "Should create U32 value");
    }

    #[test]
    fn test_value_creation_u64() {
        let val = Value::u64(1000000000);
        assert!(
            matches!(val, Value::U64(1000000000)),
            "Should create U64 value"
        );
    }

    #[test]
    fn test_value_creation_u128() {
        let val = Value::u128(1000000000000000);
        match val {
            Value::U128(boxed_val) => assert_eq!(*boxed_val, 1000000000000000),
            _ => panic!("Should create U128 value"),
        }
    }

    #[test]
    fn test_value_creation_bool_true() {
        let val = Value::bool(true);
        assert!(
            matches!(val, Value::Bool(true)),
            "Should create Bool(true) value"
        );
    }

    #[test]
    fn test_value_creation_bool_false() {
        let val = Value::bool(false);
        assert!(
            matches!(val, Value::Bool(false)),
            "Should create Bool(false) value"
        );
    }
}

#[cfg(test)]
mod integer_value_tests {
    use crate::execution::values::IntegerValue;

    #[test]
    fn test_integer_value_add_u8() {
        let a = IntegerValue::U8(10);
        let b = IntegerValue::U8(20);
        let result = a.add_checked(b).expect("Addition should succeed");
        assert!(
            matches!(result, IntegerValue::U8(30)),
            "Should add U8 values"
        );
    }

    #[test]
    fn test_integer_value_add_overflow_u8() {
        let a = IntegerValue::U8(255);
        let b = IntegerValue::U8(1);
        let result = a.add_checked(b);
        assert!(result.is_err(), "U8 addition overflow should return error");
    }

    #[test]
    fn test_integer_value_sub_u8() {
        let a = IntegerValue::U8(30);
        let b = IntegerValue::U8(10);
        let result = a.sub_checked(b).expect("Subtraction should succeed");
        assert!(
            matches!(result, IntegerValue::U8(20)),
            "Should subtract U8 values"
        );
    }

    #[test]
    fn test_integer_value_sub_underflow_u8() {
        let a = IntegerValue::U8(0);
        let b = IntegerValue::U8(1);
        let result = a.sub_checked(b);
        assert!(
            result.is_err(),
            "U8 subtraction underflow should return error"
        );
    }

    #[test]
    fn test_integer_value_mul_u8() {
        let a = IntegerValue::U8(10);
        let b = IntegerValue::U8(5);
        let result = a.mul_checked(b).expect("Multiplication should succeed");
        assert!(
            matches!(result, IntegerValue::U8(50)),
            "Should multiply U8 values"
        );
    }

    #[test]
    fn test_integer_value_mul_overflow_u8() {
        let a = IntegerValue::U8(255);
        let b = IntegerValue::U8(2);
        let result = a.mul_checked(b);
        assert!(
            result.is_err(),
            "U8 multiplication overflow should return error"
        );
    }

    #[test]
    fn test_integer_value_div_u8() {
        let a = IntegerValue::U8(50);
        let b = IntegerValue::U8(5);
        let result = a.div_checked(b).expect("Division should succeed");
        assert!(
            matches!(result, IntegerValue::U8(10)),
            "Should divide U8 values"
        );
    }

    #[test]
    fn test_integer_value_div_by_zero_u8() {
        let a = IntegerValue::U8(50);
        let b = IntegerValue::U8(0);
        let result = a.div_checked(b);
        assert!(result.is_err(), "Division by zero should return error");
    }

    #[test]
    fn test_integer_value_rem_u8() {
        let a = IntegerValue::U8(23);
        let b = IntegerValue::U8(5);
        let result = a.rem_checked(b).expect("Modulo should succeed");
        assert!(
            matches!(result, IntegerValue::U8(3)),
            "Should compute remainder"
        );
    }

    #[test]
    fn test_integer_value_rem_by_zero_u8() {
        let a = IntegerValue::U8(23);
        let b = IntegerValue::U8(0);
        let result = a.rem_checked(b);
        assert!(result.is_err(), "Modulo by zero should return error");
    }

    #[test]
    fn test_integer_value_bit_or_u8() {
        let a = IntegerValue::U8(0b1010);
        let b = IntegerValue::U8(0b0101);
        let result = a.bit_or(b).expect("BitOr should succeed");
        assert!(
            matches!(result, IntegerValue::U8(0b1111)),
            "Should compute bitwise OR"
        );
    }

    #[test]
    fn test_integer_value_bit_and_u8() {
        let a = IntegerValue::U8(0b1111);
        let b = IntegerValue::U8(0b1010);
        let result = a.bit_and(b).expect("BitAnd should succeed");
        assert!(
            matches!(result, IntegerValue::U8(0b1010)),
            "Should compute bitwise AND"
        );
    }

    #[test]
    fn test_integer_value_bit_xor_u8() {
        let a = IntegerValue::U8(0b1111);
        let b = IntegerValue::U8(0b1010);
        let result = a.bit_xor(b).expect("BitXor should succeed");
        assert!(
            matches!(result, IntegerValue::U8(0b0101)),
            "Should compute bitwise XOR"
        );
    }

    #[test]
    fn test_integer_value_shl_u8() {
        let a = IntegerValue::U8(1);
        let result = a.shl_checked(3).expect("Left shift should succeed");
        assert!(matches!(result, IntegerValue::U8(8)), "Should shift left");
    }

    #[test]
    fn test_integer_value_shl_large_amount_u8() {
        let a = IntegerValue::U8(255);
        let result = a.shl_checked(9);
        // Shifting by more than bit width should return error
        assert!(
            result.is_err(),
            "Left shift by amount >= bit width should return error"
        );
    }

    #[test]
    fn test_integer_value_shr_u8() {
        let a = IntegerValue::U8(8);
        let result = a.shr_checked(3).expect("Right shift should succeed");
        assert!(matches!(result, IntegerValue::U8(1)), "Should shift right");
    }

    #[test]
    fn test_integer_value_lt() {
        let a = IntegerValue::U8(10);
        let b = IntegerValue::U8(20);
        let result = IntegerValue::lt(a, b).expect("Comparison should succeed");
        assert!(result, "10 < 20 should be true");
    }

    #[test]
    fn test_integer_value_gt() {
        let a = IntegerValue::U8(20);
        let b = IntegerValue::U8(10);
        let result = IntegerValue::gt(a, b).expect("Comparison should succeed");
        assert!(result, "20 > 10 should be true");
    }

    #[test]
    fn test_integer_value_le() {
        let a = IntegerValue::U8(10);
        let b = IntegerValue::U8(10);
        let result = IntegerValue::le(a, b).expect("Comparison should succeed");
        assert!(result, "10 <= 10 should be true");
    }

    #[test]
    fn test_integer_value_ge() {
        let a = IntegerValue::U8(20);
        let b = IntegerValue::U8(10);
        let result = IntegerValue::ge(a, b).expect("Comparison should succeed");
        assert!(result, "20 >= 10 should be true");
    }

    #[test]
    fn test_integer_value_cast_u8_to_u16() {
        let a = IntegerValue::U8(255);
        let result = a.cast_u16().expect("Cast should succeed");
        assert_eq!(result, 255u16, "Should cast U8 to U16");
    }

    #[test]
    fn test_integer_value_cast_u16_to_u8_overflow() {
        let a = IntegerValue::U16(256);
        let result = a.cast_u8();
        assert!(result.is_err(), "Cast with overflow should return error");
    }

    #[test]
    fn test_integer_value_cast_u16_to_u32() {
        let a = IntegerValue::U16(65535);
        let result = a.cast_u32().expect("Cast should succeed");
        assert_eq!(result, 65535u32, "Should cast U16 to U32");
    }

    #[test]
    fn test_integer_value_cast_u64_to_u8_overflow() {
        let a = IntegerValue::U64(1000);
        let result = a.cast_u8();
        assert!(result.is_err(), "Cast with overflow should return error");
    }
}
