// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::accumulator_root::{AccumulatorKey, U128};
use crate::balance::Balance;
use crate::base_types::{MoveObjectType, SequenceNumber, SuiAddress};
use crate::dynamic_field::{DynamicFieldInfo, DynamicFieldKey};
use crate::gas_coin::GAS;
use crate::object::MoveObject;
use crate::{
    MoveTypeTagTrait, MoveTypeTagTraitGeneric, SUI_ACCUMULATOR_ROOT_OBJECT_ID,
    SUI_FRAMEWORK_ADDRESS,
};
use move_core_types::language_storage::{StructTag, TypeTag};

#[test]
fn test_sui_balance_accumulator_field_recognition() {
    // Create a Field<Key<Balance<SUI>>, U128> type
    let sui_balance = Balance::type_tag(GAS::type_tag());
    let key_type = AccumulatorKey::get_type_tag(&[sui_balance]);
    let u128_type = U128::get_type_tag();
    let field_type = DynamicFieldInfo::dynamic_field_type(key_type, u128_type);

    // Convert to MoveObjectType and check if it's recognized
    let move_type = MoveObjectType::from(field_type.clone());

    // Should be recognized as a balance accumulator field
    assert!(move_type.is_balance_accumulator_field());
    assert!(move_type.is_sui_balance_accumulator_field());

    // Should extract the correct balance type
    let balance_type = move_type
        .balance_accumulator_field_type_maybe()
        .expect("Should have balance type");
    assert_eq!(balance_type, GAS::type_tag());
}

#[test]
fn test_non_sui_balance_accumulator_field_recognition() {
    // Create a custom token type
    let custom_token = TypeTag::Struct(Box::new(StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: "test".parse().unwrap(),
        name: "CustomToken".parse().unwrap(),
        type_params: vec![],
    }));

    // Create a Field<Key<Balance<CustomToken>>, U128> type
    let custom_balance = Balance::type_tag(custom_token.clone());
    let key_type = AccumulatorKey::get_type_tag(&[custom_balance]);
    let u128_type = U128::get_type_tag();
    let field_type = DynamicFieldInfo::dynamic_field_type(key_type, u128_type);

    // Convert to MoveObjectType and check if it's recognized
    let move_type = MoveObjectType::from(field_type.clone());

    // Should be recognized as a balance accumulator field, but not SUI
    assert!(move_type.is_balance_accumulator_field());
    assert!(!move_type.is_sui_balance_accumulator_field());

    // Should extract the correct balance type
    let balance_type = move_type
        .balance_accumulator_field_type_maybe()
        .expect("Should have balance type");
    assert_eq!(balance_type, custom_token);
}

#[test]
fn test_non_accumulator_field_not_recognized() {
    // Create a regular dynamic field that's not an accumulator
    let key_type = TypeTag::Struct(Box::new(StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: "test".parse().unwrap(),
        name: "RegularKey".parse().unwrap(),
        type_params: vec![],
    }));
    let value_type = TypeTag::Struct(Box::new(StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: "test".parse().unwrap(),
        name: "RegularValue".parse().unwrap(),
        type_params: vec![],
    }));
    let field_type = DynamicFieldInfo::dynamic_field_type(key_type, value_type);

    // Convert to MoveObjectType and check if it's recognized
    let move_type = MoveObjectType::from(field_type.clone());

    // Should NOT be recognized as a balance accumulator field
    assert!(!move_type.is_balance_accumulator_field());
    assert!(!move_type.is_sui_balance_accumulator_field());

    // Should not extract any balance type
    assert!(move_type.balance_accumulator_field_type_maybe().is_none());
}

#[test]
fn test_accumulator_field_type_params() {
    // Create a Field<Key<Balance<SUI>>, U128> type
    let sui_balance = Balance::type_tag(GAS::type_tag());
    let key_type = AccumulatorKey::get_type_tag(&[sui_balance]);
    let u128_type = U128::get_type_tag();
    let field_type = DynamicFieldInfo::dynamic_field_type(key_type.clone(), u128_type.clone());

    // Convert to MoveObjectType
    let move_type = MoveObjectType::from(field_type.clone());

    // Check type_params returns the correct Field type parameters
    let type_params = move_type.type_params();
    assert_eq!(type_params.len(), 2);
    assert_eq!(&*type_params[0], &key_type);
    assert_eq!(&*type_params[1], &u128_type);
}

#[test]
fn test_accumulator_field_struct_tag_reconstruction() {
    // Create a Field<Key<Balance<SUI>>, U128> type
    let sui_balance = Balance::type_tag(GAS::type_tag());
    let key_type = AccumulatorKey::get_type_tag(&[sui_balance]);
    let u128_type = U128::get_type_tag();
    let original_field_type = DynamicFieldInfo::dynamic_field_type(key_type, u128_type);

    // Convert to MoveObjectType and back to StructTag
    let move_type = MoveObjectType::from(original_field_type.clone());
    let reconstructed_field_type: StructTag = move_type.into();

    // Should reconstruct the same type
    assert_eq!(original_field_type, reconstructed_field_type);
}

#[test]
fn test_accumulator_storage_savings() {
    let owner = SuiAddress::random_for_testing_only();
    let balance = 1000u64;

    // Create the field struct tag for Field<Key<Balance<SUI>>, U128>
    let sui_balance = Balance::type_tag(GAS::type_tag());
    let key_type = AccumulatorKey::get_type_tag(&[sui_balance]);
    let u128_type = U128::get_type_tag();
    let field_struct_tag = DynamicFieldInfo::dynamic_field_type(key_type, u128_type);

    // Create the field data
    let key = AccumulatorKey { owner };
    let value = U128 {
        value: balance as u128,
    };
    let field_key = DynamicFieldKey(
        SUI_ACCUMULATOR_ROOT_OBJECT_ID,
        key,
        AccumulatorKey::get_type_tag(&[GAS::type_tag()]),
    );
    let field = field_key.into_field(value).unwrap();
    let field_inner = field.into_inner();
    let field_bytes = bcs::to_bytes(&field_inner).expect("Serialization should succeed");

    // Create MoveObject using efficient type representation (our new optimization)
    let efficient_move_type = MoveObjectType::from(field_struct_tag.clone());
    let efficient_move_object = unsafe {
        MoveObject::new_from_execution_with_limit(
            efficient_move_type,
            false, // not transferable
            SequenceNumber::new(),
            field_bytes.clone(),
            512,
        )
    }
    .expect("Should create move object");

    // Create MoveObject using standard Other(StructTag) representation
    let standard_move_type =
        MoveObjectType(crate::base_types::MoveObjectType_::Other(field_struct_tag));
    let standard_move_object = unsafe {
        MoveObject::new_from_execution_with_limit(
            standard_move_type,
            false, // not transferable
            SequenceNumber::new(),
            field_bytes,
            512,
        )
    }
    .expect("Should create move object");

    // Serialize both objects and compare sizes
    let efficient_serialized = bcs::to_bytes(&efficient_move_object).expect("Should serialize");
    let standard_serialized = bcs::to_bytes(&standard_move_object).expect("Should serialize");

    println!(
        "Efficient representation size: {} bytes",
        efficient_serialized.len()
    );
    println!(
        "Standard representation size: {} bytes",
        standard_serialized.len()
    );
    println!(
        "Savings: {} bytes",
        standard_serialized.len() - efficient_serialized.len()
    );

    // Verify that the efficient representation is significantly smaller
    assert!(
        efficient_serialized.len() < standard_serialized.len(),
        "Efficient representation should be smaller than standard representation"
    );

    // Expect substantial savings (at least 100 bytes based on design document estimates)
    let savings = standard_serialized.len() - efficient_serialized.len();
    assert!(
        savings >= 100,
        "Expected at least 100 bytes of savings, got {} bytes",
        savings
    );

    // Also verify the efficient object is using the right type
    assert!(
        efficient_move_object
            .type_()
            .is_sui_balance_accumulator_field()
    );
}
