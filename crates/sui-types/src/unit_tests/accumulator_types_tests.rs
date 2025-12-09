// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::accumulator_metadata::{AccumulatorMetadata, AccumulatorOwner, MetadataKey, OwnerKey};
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

// ============================================================================
// Tests for Balance Accumulator Metadata Fields
// ============================================================================

#[test]
fn test_sui_balance_accumulator_metadata_field_recognition() {
    // Create a Field<MetadataKey<Balance<SUI>>, Metadata<Balance<SUI>>> type
    let sui_balance = Balance::type_tag(GAS::type_tag());
    let key_type = MetadataKey::get_type_tag(std::slice::from_ref(&sui_balance));
    let metadata_type = AccumulatorMetadata::get_type_tag(&[sui_balance]);
    let field_type = DynamicFieldInfo::dynamic_field_type(key_type, metadata_type);

    // Convert to MoveObjectType and check if it's recognized
    let move_type = MoveObjectType::from(field_type.clone());

    // Should be recognized as a balance accumulator metadata field
    assert!(move_type.is_balance_accumulator_metadata_field());

    // Should extract the correct balance type (which should be SUI)
    let balance_type = move_type
        .balance_accumulator_metadata_field_type_maybe()
        .expect("Should have balance type");
    assert_eq!(balance_type, GAS::type_tag());

    // Since the balance type is SUI, this should be a SuiBalanceAccumulatorMetadataField
    assert!(
        matches!(
            move_type.0,
            crate::base_types::MoveObjectType_::SuiBalanceAccumulatorMetadataField
        ),
        "Should be SuiBalanceAccumulatorMetadataField variant"
    );
}

#[test]
fn test_non_sui_balance_accumulator_metadata_field_recognition() {
    // Create a custom token type
    let custom_token = TypeTag::Struct(Box::new(StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: "test".parse().unwrap(),
        name: "CustomToken".parse().unwrap(),
        type_params: vec![],
    }));

    // Create a Field<MetadataKey<Balance<CustomToken>>, Metadata<Balance<CustomToken>>> type
    let custom_balance = Balance::type_tag(custom_token.clone());
    let key_type = MetadataKey::get_type_tag(std::slice::from_ref(&custom_balance));
    let metadata_type = AccumulatorMetadata::get_type_tag(&[custom_balance]);
    let field_type = DynamicFieldInfo::dynamic_field_type(key_type, metadata_type);

    // Convert to MoveObjectType and check if it's recognized
    let move_type = MoveObjectType::from(field_type.clone());

    // Should be recognized as a balance accumulator metadata field, but not SUI
    assert!(move_type.is_balance_accumulator_metadata_field());

    // Should extract the correct balance type
    let balance_type = move_type
        .balance_accumulator_metadata_field_type_maybe()
        .expect("Should have balance type");
    assert_eq!(balance_type, custom_token);

    // Since the balance type is not SUI, this should be a BalanceAccumulatorMetadataField
    assert!(
        matches!(
            move_type.0,
            crate::base_types::MoveObjectType_::BalanceAccumulatorMetadataField(_)
        ),
        "Should be BalanceAccumulatorMetadataField variant"
    );
}

#[test]
fn test_non_accumulator_metadata_field_not_recognized() {
    // Create a regular dynamic field that's not an accumulator metadata field
    let key_type = TypeTag::Struct(Box::new(StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: "test".parse().unwrap(),
        name: "RegularMetadataKey".parse().unwrap(),
        type_params: vec![],
    }));
    let value_type = TypeTag::Struct(Box::new(StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: "test".parse().unwrap(),
        name: "RegularMetadataValue".parse().unwrap(),
        type_params: vec![],
    }));
    let field_type = DynamicFieldInfo::dynamic_field_type(key_type, value_type);

    // Convert to MoveObjectType and check if it's recognized
    let move_type = MoveObjectType::from(field_type.clone());

    // Should NOT be recognized as a balance accumulator metadata field
    assert!(!move_type.is_balance_accumulator_metadata_field());

    // Should not extract any balance type
    assert!(
        move_type
            .balance_accumulator_metadata_field_type_maybe()
            .is_none()
    );
}

#[test]
fn test_accumulator_metadata_field_type_params() {
    // Create a Field<MetadataKey<Balance<SUI>>, Metadata<Balance<SUI>>> type
    let sui_balance = Balance::type_tag(GAS::type_tag());
    let key_type = MetadataKey::get_type_tag(std::slice::from_ref(&sui_balance));
    let metadata_type = AccumulatorMetadata::get_type_tag(&[sui_balance]);
    let field_type = DynamicFieldInfo::dynamic_field_type(key_type.clone(), metadata_type.clone());

    // Convert to MoveObjectType
    let move_type = MoveObjectType::from(field_type.clone());

    // Check type_params returns the correct Field type parameters
    let type_params = move_type.type_params();
    assert_eq!(type_params.len(), 2);
    assert_eq!(*type_params[0], key_type);
    assert_eq!(*type_params[1], metadata_type);
}

#[test]
fn test_accumulator_owner_field_type_params() {
    // Create a Field<OwnerKey, AccumulatorOwner> type
    let key_type = OwnerKey::get_type_tag();
    let owner_type = AccumulatorOwner::get_type_tag();
    let field_type = DynamicFieldInfo::dynamic_field_type(key_type.clone(), owner_type.clone());

    // Convert to MoveObjectType
    let move_type = MoveObjectType::from(field_type.clone());

    // Check type_params returns the correct Field type parameters
    let type_params = move_type.type_params();
    assert_eq!(type_params.len(), 2);
    assert_eq!(*type_params[0], key_type);
    assert_eq!(*type_params[1], owner_type);
}

#[test]
fn test_accumulator_metadata_field_struct_tag_reconstruction() {
    // Create a Field<MetadataKey<Balance<SUI>>, Metadata<Balance<SUI>>> type
    let sui_balance = Balance::type_tag(GAS::type_tag());
    let key_type = MetadataKey::get_type_tag(std::slice::from_ref(&sui_balance));
    let metadata_type = AccumulatorMetadata::get_type_tag(&[sui_balance]);
    let original_field_type = DynamicFieldInfo::dynamic_field_type(key_type, metadata_type);

    // Convert to MoveObjectType and back to StructTag
    let move_type = MoveObjectType::from(original_field_type.clone());
    let reconstructed_field_type: StructTag = move_type.into();

    // Should reconstruct the same type
    assert_eq!(original_field_type, reconstructed_field_type);
}

#[test]
fn test_accumulator_owner_field_struct_tag_reconstruction() {
    // Create a Field<OwnerKey, AccumulatorOwner> type
    let key_type = OwnerKey::get_type_tag();
    let owner_type = AccumulatorOwner::get_type_tag();
    let original_field_type = DynamicFieldInfo::dynamic_field_type(key_type, owner_type);

    // Convert to MoveObjectType and back to StructTag
    let move_type = MoveObjectType::from(original_field_type.clone());
    let reconstructed_field_type: StructTag = move_type.into();

    // Should reconstruct the same type
    assert_eq!(original_field_type, reconstructed_field_type);
}

#[test]
fn test_accumulator_metadata_storage_savings() {
    // Create the field struct tag for Field<MetadataKey<Balance<SUI>>, Metadata<Balance<SUI>>>
    let sui_balance = Balance::type_tag(GAS::type_tag());
    let key_type = MetadataKey::get_type_tag(std::slice::from_ref(&sui_balance));
    let metadata_type = AccumulatorMetadata::get_type_tag(&[sui_balance]);
    let field_struct_tag = DynamicFieldInfo::dynamic_field_type(key_type, metadata_type);

    // The main savings come from the type representation itself
    // Compare the efficient enum variant vs the full StructTag

    // Create the efficient representation
    let efficient_move_type = MoveObjectType::from(field_struct_tag.clone());

    // Create the standard Other(StructTag) representation
    let standard_move_type = MoveObjectType(crate::base_types::MoveObjectType_::Other(
        field_struct_tag.clone(),
    ));

    // Serialize just the types to compare their sizes
    let efficient_type_serialized = bcs::to_bytes(&efficient_move_type).expect("Should serialize");
    let standard_type_serialized = bcs::to_bytes(&standard_move_type).expect("Should serialize");

    println!(
        "Efficient metadata type representation size: {} bytes",
        efficient_type_serialized.len()
    );
    println!(
        "Standard metadata type representation size: {} bytes",
        standard_type_serialized.len()
    );
    println!(
        "Metadata type savings: {} bytes",
        standard_type_serialized.len() - efficient_type_serialized.len()
    );

    // Verify that the efficient representation is significantly smaller
    assert!(
        efficient_type_serialized.len() < standard_type_serialized.len(),
        "Efficient representation should be smaller than standard representation"
    );

    // The type alone should save substantial space
    let savings = standard_type_serialized.len() - efficient_type_serialized.len();
    assert!(
        savings >= 100,
        "Expected at least 100 bytes of savings in type representation, got {} bytes",
        savings
    );

    // Also verify we get the right type back
    assert!(
        efficient_move_type.is_balance_accumulator_metadata_field(),
        "Should be recognized as balance accumulator metadata field"
    );

    // Specifically verify it's the SUI variant
    assert!(
        matches!(
            efficient_move_type.0,
            crate::base_types::MoveObjectType_::SuiBalanceAccumulatorMetadataField
        ),
        "Should be SuiBalanceAccumulatorMetadataField variant"
    );

    // Verify round-trip conversion
    let reconstructed: StructTag = efficient_move_type.into();
    assert_eq!(
        reconstructed, field_struct_tag,
        "Round-trip conversion should preserve the type"
    );
}

#[test]
fn test_accumulator_owner_field_storage_savings() {
    let key_type = OwnerKey::get_type_tag();
    let value_type = AccumulatorOwner::get_type_tag();
    let field_struct_tag = DynamicFieldInfo::dynamic_field_type(key_type, value_type);

    // Create the efficient representation
    let efficient_move_type = MoveObjectType::from(field_struct_tag.clone());

    // Create the standard representation
    let standard_move_type = MoveObjectType(crate::base_types::MoveObjectType_::Other(
        field_struct_tag.clone(),
    ));

    let efficient_type_serialized = bcs::to_bytes(&efficient_move_type).expect("Should serialize");
    let standard_type_serialized = bcs::to_bytes(&standard_move_type).expect("Should serialize");

    // Print size comparison for manual inspection
    println!(
        "Efficient owner field type representation size: {} bytes",
        efficient_type_serialized.len()
    );
    println!(
        "Standard owner field type representation size: {} bytes",
        standard_type_serialized.len()
    );
    println!(
        "Owner field type savings: {} bytes",
        standard_type_serialized.len() - efficient_type_serialized.len()
    );

    // Efficient type should be smaller
    assert!(
        efficient_type_serialized.len() < standard_type_serialized.len(),
        "Efficient owner type should be smaller than standard type"
    );
    // Expect substantial savings—should be at least 100 bytes as in other tests
    let savings = standard_type_serialized.len() - efficient_type_serialized.len();
    assert!(
        savings >= 100,
        "Expected at least 100 bytes of savings in owner field type representation, got {} bytes",
        savings
    );
    // Also verify correct MoveObjectType variant is detected
    assert!(
        efficient_move_type.is_balance_accumulator_owner_field(),
        "Should be recognized as balance accumulator owner field"
    );
    assert!(
        matches!(
            efficient_move_type.0,
            crate::base_types::MoveObjectType_::BalanceAccumulatorOwnerField
        ),
        "Should be BalanceAccumulatorOwnerField variant"
    );
    // Verify round-trip conversion
    let reconstructed: StructTag = efficient_move_type.into();
    assert_eq!(
        reconstructed, field_struct_tag,
        "Round-trip conversion should preserve the type"
    );
}

// Below this point are a bunch of claude-written tests. They aren't terribly high value
// but they are small and fast and do check actual edge cases.

#[test]
fn test_accumulator_field_wrong_value_type_not_u128() {
    // Field<Key<Balance<SUI>>, SomeOtherType> should NOT be recognized
    let sui_balance = Balance::type_tag(GAS::type_tag());
    let key_type = AccumulatorKey::get_type_tag(&[sui_balance]);

    // Use a different value type (not U128)
    let wrong_value_type = TypeTag::Struct(Box::new(StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: "test".parse().unwrap(),
        name: "NotU128".parse().unwrap(),
        type_params: vec![],
    }));

    let field_type = DynamicFieldInfo::dynamic_field_type(key_type, wrong_value_type);
    let move_type = MoveObjectType::from(field_type.clone());

    assert!(!move_type.is_balance_accumulator_field());
    assert!(move_type.balance_accumulator_field_type_maybe().is_none());
    assert_eq!(StructTag::from(move_type), field_type);
}

#[test]
fn test_accumulator_field_key_not_balance_type() {
    // Field<Key<SomeOtherType>, U128> should NOT be recognized (Key type param isn't Balance<T>)
    let not_balance = TypeTag::Struct(Box::new(StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: "test".parse().unwrap(),
        name: "NotBalance".parse().unwrap(),
        type_params: vec![],
    }));
    let key_type = AccumulatorKey::get_type_tag(&[not_balance]);
    let u128_type = U128::get_type_tag();

    let field_type = DynamicFieldInfo::dynamic_field_type(key_type, u128_type);
    let move_type = MoveObjectType::from(field_type.clone());

    assert!(!move_type.is_balance_accumulator_field());
    assert!(move_type.balance_accumulator_field_type_maybe().is_none());
    assert_eq!(StructTag::from(move_type), field_type);
}

#[test]
fn test_accumulator_field_wrong_key_module() {
    // Key from wrong module should not be recognized
    let sui_balance = Balance::type_tag(GAS::type_tag());
    let wrong_key_type = TypeTag::Struct(Box::new(StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: "wrong_module".parse().unwrap(),
        name: "Key".parse().unwrap(),
        type_params: vec![sui_balance],
    }));
    let u128_type = U128::get_type_tag();

    let field_type = DynamicFieldInfo::dynamic_field_type(wrong_key_type, u128_type);
    let move_type = MoveObjectType::from(field_type.clone());

    assert!(!move_type.is_balance_accumulator_field());
    assert_eq!(StructTag::from(move_type), field_type);
}

#[test]
fn test_accumulator_field_wrong_address() {
    // Key from wrong address should not be recognized
    use move_core_types::account_address::AccountAddress;

    let sui_balance = Balance::type_tag(GAS::type_tag());
    let wrong_key_type = TypeTag::Struct(Box::new(StructTag {
        address: AccountAddress::from_hex_literal("0x999").unwrap(),
        module: "accumulator".parse().unwrap(),
        name: "Key".parse().unwrap(),
        type_params: vec![sui_balance],
    }));
    let u128_type = U128::get_type_tag();

    let field_type = DynamicFieldInfo::dynamic_field_type(wrong_key_type, u128_type);
    let move_type = MoveObjectType::from(field_type.clone());

    assert!(!move_type.is_balance_accumulator_field());
    assert_eq!(StructTag::from(move_type), field_type);
}

#[test]
fn test_accumulator_field_key_with_no_type_params() {
    // Key<> with no type params should not be recognized
    let empty_key_type = TypeTag::Struct(Box::new(StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: "accumulator".parse().unwrap(),
        name: "Key".parse().unwrap(),
        type_params: vec![],
    }));
    let u128_type = U128::get_type_tag();

    let field_type = DynamicFieldInfo::dynamic_field_type(empty_key_type, u128_type);
    let move_type = MoveObjectType::from(field_type.clone());

    assert!(!move_type.is_balance_accumulator_field());
    assert_eq!(StructTag::from(move_type), field_type);
}

#[test]
fn test_accumulator_field_key_with_multiple_type_params() {
    // Key<Balance<SUI>, ExtraParam> should not be recognized
    let sui_balance = Balance::type_tag(GAS::type_tag());
    let extra_param = TypeTag::U64;
    let key_type = TypeTag::Struct(Box::new(StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: "accumulator".parse().unwrap(),
        name: "Key".parse().unwrap(),
        type_params: vec![sui_balance, extra_param],
    }));
    let u128_type = U128::get_type_tag();

    let field_type = DynamicFieldInfo::dynamic_field_type(key_type, u128_type);
    let move_type = MoveObjectType::from(field_type.clone());

    assert!(!move_type.is_balance_accumulator_field());
    assert_eq!(StructTag::from(move_type), field_type);
}

#[test]
fn test_accumulator_field_non_struct_key() {
    // Field<u64, U128> should not be recognized as accumulator field
    let key_type = TypeTag::U64;
    let u128_type = U128::get_type_tag();

    let field_type = DynamicFieldInfo::dynamic_field_type(key_type, u128_type);
    let move_type = MoveObjectType::from(field_type.clone());

    assert!(!move_type.is_balance_accumulator_field());
    assert_eq!(StructTag::from(move_type), field_type);
}

#[test]
fn test_accumulator_field_with_nested_balance_type() {
    // Balance<Balance<SUI>> - nested balance should still work correctly
    let inner_balance = Balance::type_tag(GAS::type_tag());
    let nested_balance = Balance::type_tag(inner_balance.clone());
    let key_type = AccumulatorKey::get_type_tag(std::slice::from_ref(&nested_balance));
    let u128_type = U128::get_type_tag();

    let field_type = DynamicFieldInfo::dynamic_field_type(key_type, u128_type);
    let move_type = MoveObjectType::from(field_type.clone());

    // Should recognize as accumulator field with Balance<Balance<SUI>> as the extracted type
    assert!(move_type.is_balance_accumulator_field());
    let extracted = move_type.balance_accumulator_field_type_maybe().unwrap();
    // The extracted type should be Balance<SUI>, which is the inner type of Balance<Balance<SUI>>
    assert_eq!(extracted, inner_balance);
    assert_eq!(StructTag::from(move_type), field_type);
}

// ============================================================================
// Edge Case Tests for Metadata Field Recognition
// ============================================================================

#[test]
fn test_metadata_field_mismatched_key_value_types() {
    // Field<MetadataKey<Balance<SUI>>, Metadata<Balance<CustomToken>>> should NOT be recognized
    // because the key and value have different type parameters
    let sui_balance = Balance::type_tag(GAS::type_tag());
    let custom_token = TypeTag::Struct(Box::new(StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: "test".parse().unwrap(),
        name: "CustomToken".parse().unwrap(),
        type_params: vec![],
    }));
    let custom_balance = Balance::type_tag(custom_token);

    let key_type = MetadataKey::get_type_tag(std::slice::from_ref(&sui_balance));
    let metadata_type = AccumulatorMetadata::get_type_tag(&[custom_balance]);

    let field_type = DynamicFieldInfo::dynamic_field_type(key_type, metadata_type);
    let move_type = MoveObjectType::from(field_type.clone());

    // Should NOT be recognized because type params don't match
    assert!(!move_type.is_balance_accumulator_metadata_field());
    assert!(
        move_type
            .balance_accumulator_metadata_field_type_maybe()
            .is_none()
    );
    assert_eq!(StructTag::from(move_type), field_type);
}

#[test]
fn test_metadata_field_key_not_balance() {
    // Field<MetadataKey<NotBalance>, Metadata<NotBalance>> should NOT be recognized
    let not_balance = TypeTag::Struct(Box::new(StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: "test".parse().unwrap(),
        name: "NotBalance".parse().unwrap(),
        type_params: vec![],
    }));

    let key_type = MetadataKey::get_type_tag(std::slice::from_ref(&not_balance));
    let metadata_type = AccumulatorMetadata::get_type_tag(&[not_balance]);

    let field_type = DynamicFieldInfo::dynamic_field_type(key_type, metadata_type);
    let move_type = MoveObjectType::from(field_type.clone());

    // Should NOT be recognized because inner type is not Balance<T>
    assert!(!move_type.is_balance_accumulator_metadata_field());
    assert_eq!(StructTag::from(move_type), field_type);
}

#[test]
fn test_metadata_field_wrong_key_module() {
    let sui_balance = Balance::type_tag(GAS::type_tag());

    // MetadataKey from wrong module
    let wrong_key_type = TypeTag::Struct(Box::new(StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: "wrong_module".parse().unwrap(),
        name: "MetadataKey".parse().unwrap(),
        type_params: vec![sui_balance.clone()],
    }));
    let metadata_type = AccumulatorMetadata::get_type_tag(&[sui_balance]);

    let field_type = DynamicFieldInfo::dynamic_field_type(wrong_key_type, metadata_type);
    let move_type = MoveObjectType::from(field_type.clone());

    assert!(!move_type.is_balance_accumulator_metadata_field());
    assert_eq!(StructTag::from(move_type), field_type);
}

#[test]
fn test_metadata_field_wrong_value_module() {
    let sui_balance = Balance::type_tag(GAS::type_tag());

    let key_type = MetadataKey::get_type_tag(std::slice::from_ref(&sui_balance));
    // Metadata from wrong module
    let wrong_metadata_type = TypeTag::Struct(Box::new(StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: "wrong_module".parse().unwrap(),
        name: "Metadata".parse().unwrap(),
        type_params: vec![sui_balance],
    }));

    let field_type = DynamicFieldInfo::dynamic_field_type(key_type, wrong_metadata_type);
    let move_type = MoveObjectType::from(field_type.clone());

    assert!(!move_type.is_balance_accumulator_metadata_field());
    assert_eq!(StructTag::from(move_type), field_type);
}

#[test]
fn test_metadata_field_key_no_type_params() {
    let sui_balance = Balance::type_tag(GAS::type_tag());

    // MetadataKey with no type params
    let key_type = TypeTag::Struct(Box::new(StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: "accumulator_metadata".parse().unwrap(),
        name: "MetadataKey".parse().unwrap(),
        type_params: vec![],
    }));
    let metadata_type = AccumulatorMetadata::get_type_tag(&[sui_balance]);

    let field_type = DynamicFieldInfo::dynamic_field_type(key_type, metadata_type);
    let move_type = MoveObjectType::from(field_type.clone());

    assert!(!move_type.is_balance_accumulator_metadata_field());
    assert_eq!(StructTag::from(move_type), field_type);
}

#[test]
fn test_metadata_field_value_no_type_params() {
    let sui_balance = Balance::type_tag(GAS::type_tag());

    let key_type = MetadataKey::get_type_tag(std::slice::from_ref(&sui_balance));
    // Metadata with no type params
    let metadata_type = TypeTag::Struct(Box::new(StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: "accumulator_metadata".parse().unwrap(),
        name: "Metadata".parse().unwrap(),
        type_params: vec![],
    }));

    let field_type = DynamicFieldInfo::dynamic_field_type(key_type, metadata_type);
    let move_type = MoveObjectType::from(field_type.clone());

    assert!(!move_type.is_balance_accumulator_metadata_field());
    assert_eq!(StructTag::from(move_type), field_type);
}

#[test]
fn test_metadata_field_non_struct_types() {
    // Field<u64, u64> should not be recognized as metadata field
    let key_type = TypeTag::U64;
    let value_type = TypeTag::U64;

    let field_type = DynamicFieldInfo::dynamic_field_type(key_type, value_type);
    let move_type = MoveObjectType::from(field_type.clone());

    assert!(!move_type.is_balance_accumulator_metadata_field());
    assert_eq!(StructTag::from(move_type), field_type);
}

// ============================================================================
// Edge Case Tests for Owner Field Recognition
// ============================================================================

#[test]
fn test_owner_field_wrong_key_type() {
    // Field<WrongKey, AccumulatorOwner> should NOT be recognized
    let wrong_key = TypeTag::Struct(Box::new(StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: "test".parse().unwrap(),
        name: "WrongKey".parse().unwrap(),
        type_params: vec![],
    }));
    let owner_type = AccumulatorOwner::get_type_tag();

    let field_type = DynamicFieldInfo::dynamic_field_type(wrong_key, owner_type);
    let move_type = MoveObjectType::from(field_type.clone());

    assert!(!move_type.is_balance_accumulator_owner_field());
    assert_eq!(StructTag::from(move_type), field_type);
}

#[test]
fn test_owner_field_wrong_value_type() {
    // Field<OwnerKey, WrongValue> should NOT be recognized
    let key_type = OwnerKey::get_type_tag();
    let wrong_value = TypeTag::Struct(Box::new(StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: "test".parse().unwrap(),
        name: "WrongValue".parse().unwrap(),
        type_params: vec![],
    }));

    let field_type = DynamicFieldInfo::dynamic_field_type(key_type, wrong_value);
    let move_type = MoveObjectType::from(field_type.clone());

    assert!(!move_type.is_balance_accumulator_owner_field());
    assert_eq!(StructTag::from(move_type), field_type);
}

#[test]
fn test_owner_field_key_with_type_params() {
    // OwnerKey with unexpected type params should not match
    let key_type = TypeTag::Struct(Box::new(StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: "accumulator_metadata".parse().unwrap(),
        name: "OwnerKey".parse().unwrap(),
        type_params: vec![TypeTag::U64], // OwnerKey should have no type params
    }));
    let owner_type = AccumulatorOwner::get_type_tag();

    let field_type = DynamicFieldInfo::dynamic_field_type(key_type, owner_type);
    let move_type = MoveObjectType::from(field_type.clone());

    assert!(!move_type.is_balance_accumulator_owner_field());
    assert_eq!(StructTag::from(move_type), field_type);
}

#[test]
fn test_owner_field_value_with_type_params() {
    // AccumulatorOwner with unexpected type params should not match
    let key_type = OwnerKey::get_type_tag();
    let owner_type = TypeTag::Struct(Box::new(StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: "accumulator_metadata".parse().unwrap(),
        name: "AccumulatorOwner".parse().unwrap(),
        type_params: vec![TypeTag::U64], // AccumulatorOwner should have no type params
    }));

    let field_type = DynamicFieldInfo::dynamic_field_type(key_type, owner_type);
    let move_type = MoveObjectType::from(field_type.clone());

    assert!(!move_type.is_balance_accumulator_owner_field());
    assert_eq!(StructTag::from(move_type), field_type);
}

#[test]
fn test_owner_field_wrong_address() {
    use move_core_types::account_address::AccountAddress;

    // OwnerKey from wrong address
    let key_type = TypeTag::Struct(Box::new(StructTag {
        address: AccountAddress::from_hex_literal("0x999").unwrap(),
        module: "accumulator_metadata".parse().unwrap(),
        name: "OwnerKey".parse().unwrap(),
        type_params: vec![],
    }));
    let owner_type = AccumulatorOwner::get_type_tag();

    let field_type = DynamicFieldInfo::dynamic_field_type(key_type, owner_type);
    let move_type = MoveObjectType::from(field_type.clone());

    assert!(!move_type.is_balance_accumulator_owner_field());
    assert_eq!(StructTag::from(move_type), field_type);
}

// ============================================================================
// Edge Case Tests for Field Structure
// ============================================================================

#[test]
fn test_not_a_field_struct() {
    // A struct that's not a Field should not be recognized
    let not_field = StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: "test".parse().unwrap(),
        name: "NotField".parse().unwrap(),
        type_params: vec![],
    };

    let move_type = MoveObjectType::from(not_field.clone());

    assert!(!move_type.is_balance_accumulator_field());
    assert!(!move_type.is_balance_accumulator_metadata_field());
    assert!(!move_type.is_balance_accumulator_owner_field());
    assert_eq!(StructTag::from(move_type), not_field);
}

#[test]
fn test_field_with_one_type_param() {
    // Field<T> with only one type param should not be recognized
    let sui_balance = Balance::type_tag(GAS::type_tag());
    let key_type = AccumulatorKey::get_type_tag(&[sui_balance]);

    let field_type = StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: "dynamic_field".parse().unwrap(),
        name: "Field".parse().unwrap(),
        type_params: vec![key_type], // Only one type param
    };

    let move_type = MoveObjectType::from(field_type.clone());

    assert!(!move_type.is_balance_accumulator_field());
    assert_eq!(StructTag::from(move_type), field_type);
}

#[test]
fn test_field_with_three_type_params() {
    // Field<K, V, Extra> with three type params should not be recognized
    let sui_balance = Balance::type_tag(GAS::type_tag());
    let key_type = AccumulatorKey::get_type_tag(&[sui_balance]);
    let u128_type = U128::get_type_tag();

    let field_type = StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: "dynamic_field".parse().unwrap(),
        name: "Field".parse().unwrap(),
        type_params: vec![key_type, u128_type, TypeTag::U64], // Three type params
    };

    let move_type = MoveObjectType::from(field_type.clone());

    assert!(!move_type.is_balance_accumulator_field());
    assert_eq!(StructTag::from(move_type), field_type);
}

#[test]
fn test_field_from_wrong_module() {
    // Field from wrong module should not be recognized
    let sui_balance = Balance::type_tag(GAS::type_tag());
    let key_type = AccumulatorKey::get_type_tag(&[sui_balance]);
    let u128_type = U128::get_type_tag();

    let field_type = StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: "wrong_module".parse().unwrap(),
        name: "Field".parse().unwrap(),
        type_params: vec![key_type, u128_type],
    };

    let move_type = MoveObjectType::from(field_type.clone());

    assert!(!move_type.is_balance_accumulator_field());
    assert_eq!(StructTag::from(move_type), field_type);
}

// ============================================================================
// Tests for MoveObjectType::is() method
// ============================================================================

#[test]
fn test_is_method_sui_balance_accumulator_field() {
    let sui_balance = Balance::type_tag(GAS::type_tag());
    let key_type = AccumulatorKey::get_type_tag(&[sui_balance]);
    let u128_type = U128::get_type_tag();
    let field_type = DynamicFieldInfo::dynamic_field_type(key_type, u128_type);

    let move_type = MoveObjectType::from(field_type.clone());

    // is() should return true for the same struct tag
    assert!(move_type.is(&field_type));

    // is() should return false for a different struct tag
    let different_field = DynamicFieldInfo::dynamic_field_type(TypeTag::U64, TypeTag::U64);
    assert!(!move_type.is(&different_field));

    assert_eq!(StructTag::from(move_type), field_type);
}

#[test]
fn test_is_method_non_sui_balance_accumulator_field() {
    let custom_token = TypeTag::Struct(Box::new(StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: "test".parse().unwrap(),
        name: "CustomToken".parse().unwrap(),
        type_params: vec![],
    }));
    let custom_balance = Balance::type_tag(custom_token.clone());
    let key_type = AccumulatorKey::get_type_tag(&[custom_balance]);
    let u128_type = U128::get_type_tag();
    let field_type = DynamicFieldInfo::dynamic_field_type(key_type, u128_type);

    let move_type = MoveObjectType::from(field_type.clone());

    assert!(move_type.is(&field_type));

    // Should not match SUI accumulator field
    let sui_balance = Balance::type_tag(GAS::type_tag());
    let sui_key_type = AccumulatorKey::get_type_tag(&[sui_balance]);
    let sui_field_type = DynamicFieldInfo::dynamic_field_type(sui_key_type, U128::get_type_tag());
    assert!(!move_type.is(&sui_field_type));

    assert_eq!(StructTag::from(move_type), field_type);
}

#[test]
fn test_is_method_metadata_field() {
    let sui_balance = Balance::type_tag(GAS::type_tag());
    let key_type = MetadataKey::get_type_tag(std::slice::from_ref(&sui_balance));
    let metadata_type = AccumulatorMetadata::get_type_tag(&[sui_balance]);
    let field_type = DynamicFieldInfo::dynamic_field_type(key_type, metadata_type);

    let move_type = MoveObjectType::from(field_type.clone());

    assert!(move_type.is(&field_type));
    assert_eq!(StructTag::from(move_type), field_type);
}

#[test]
fn test_is_method_owner_field() {
    let key_type = OwnerKey::get_type_tag();
    let owner_type = AccumulatorOwner::get_type_tag();
    let field_type = DynamicFieldInfo::dynamic_field_type(key_type, owner_type);

    let move_type = MoveObjectType::from(field_type.clone());

    assert!(move_type.is(&field_type));
    assert_eq!(StructTag::from(move_type), field_type);
}

// ============================================================================
// Tests for complex/nested type parameters
// ============================================================================

#[test]
fn test_balance_with_generic_type_param() {
    // Balance<vector<u8>> as the inner token type
    let vector_u8 = TypeTag::Vector(Box::new(TypeTag::U8));
    let balance_of_vector = Balance::type_tag(vector_u8.clone());
    let key_type = AccumulatorKey::get_type_tag(&[balance_of_vector]);
    let u128_type = U128::get_type_tag();

    let field_type = DynamicFieldInfo::dynamic_field_type(key_type, u128_type);
    let move_type = MoveObjectType::from(field_type.clone());

    assert!(move_type.is_balance_accumulator_field());
    let extracted = move_type.balance_accumulator_field_type_maybe().unwrap();
    assert_eq!(extracted, vector_u8);

    // Round-trip
    let reconstructed: StructTag = move_type.into();
    assert_eq!(reconstructed, field_type);
}

#[test]
fn test_metadata_with_generic_type_param() {
    // Metadata field with Balance<vector<u8>> as inner type
    let vector_u8 = TypeTag::Vector(Box::new(TypeTag::U8));
    let balance_of_vector = Balance::type_tag(vector_u8.clone());
    let key_type = MetadataKey::get_type_tag(std::slice::from_ref(&balance_of_vector));
    let metadata_type = AccumulatorMetadata::get_type_tag(&[balance_of_vector]);

    let field_type = DynamicFieldInfo::dynamic_field_type(key_type, metadata_type);
    let move_type = MoveObjectType::from(field_type.clone());

    assert!(move_type.is_balance_accumulator_metadata_field());
    let extracted = move_type
        .balance_accumulator_metadata_field_type_maybe()
        .unwrap();
    assert_eq!(extracted, vector_u8);

    // Round-trip
    let reconstructed: StructTag = move_type.into();
    assert_eq!(reconstructed, field_type);
}
