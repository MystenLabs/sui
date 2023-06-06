// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    account_address::AccountAddress,
    gas_algebra::AbstractMemorySize,
    identifier::{IdentStr, Identifier},
    language_storage::{ModuleId, StructTag, TypeTag, TYPETAG_ENUM_ABSTRACT_SIZE},
};
use bcs::test_helpers::assert_canonical_encode_decode;
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_module_id_canonical_roundtrip(module_id in any::<ModuleId>()) {
        assert_canonical_encode_decode(module_id);
    }
}

#[test]
fn test_type_tag_abstract_size() {
    // size = TYPETAG_ENUM_ABSTRACT_SIZE + TYPETAG_ENUM_ABSTRACT_SIZE
    let type_tag1 = TypeTag::Vector(Box::new(TypeTag::U16));
    // size = TYPETAG_ENUM_ABSTRACT_SIZE + TYPETAG_ENUM_ABSTRACT_SIZE
    let type_tag2 = TypeTag::Vector(Box::new(TypeTag::U256));
    // size = TYPETAG_ENUM_ABSTRACT_SIZE + TYPETAG_ENUM_ABSTRACT_SIZE
    let type_tag3 = TypeTag::Vector(Box::new(TypeTag::U128));

    assert_eq!(
        type_tag1.abstract_size_for_gas_metering(),
        *TYPETAG_ENUM_ABSTRACT_SIZE + *TYPETAG_ENUM_ABSTRACT_SIZE
    );
    assert_eq!(
        type_tag2.abstract_size_for_gas_metering(),
        *TYPETAG_ENUM_ABSTRACT_SIZE + *TYPETAG_ENUM_ABSTRACT_SIZE
    );
    assert_eq!(
        type_tag3.abstract_size_for_gas_metering(),
        *TYPETAG_ENUM_ABSTRACT_SIZE + *TYPETAG_ENUM_ABSTRACT_SIZE
    );

    let struct_tag1 = StructTag {
        // size = AccountAddress::LENGTH
        address: AccountAddress::ONE,
        // size = 10
        module: Identifier::from(IdentStr::new("TestModule").unwrap()),
        // size = 10
        name: Identifier::from(IdentStr::new("TestStruct").unwrap()),
        type_params: vec![
            // size = TYPETAG_ENUM_ABSTRACT_SIZE
            TypeTag::U8,
            // size = TYPETAG_ENUM_ABSTRACT_SIZE
            TypeTag::U16,
            // size = TYPETAG_ENUM_ABSTRACT_SIZE
            TypeTag::U32,
            // size = TYPETAG_ENUM_ABSTRACT_SIZE
            TypeTag::U64,
            // size = TYPETAG_ENUM_ABSTRACT_SIZE
            TypeTag::U128,
            // size = TYPETAG_ENUM_ABSTRACT_SIZE
            TypeTag::U256,
            // size = TYPETAG_ENUM_ABSTRACT_SIZE
            TypeTag::Bool,
            // size = TYPETAG_ENUM_ABSTRACT_SIZE
            TypeTag::Address,
            // size = TYPETAG_ENUM_ABSTRACT_SIZE
            TypeTag::Signer,
            // size = TYPETAG_ENUM_ABSTRACT_SIZE + TYPETAG_ENUM_ABSTRACT_SIZE
            type_tag1,
            // size = TYPETAG_ENUM_ABSTRACT_SIZE + TYPETAG_ENUM_ABSTRACT_SIZE
            type_tag2,
            // size = TYPETAG_ENUM_ABSTRACT_SIZE + TYPETAG_ENUM_ABSTRACT_SIZE
            type_tag3,
        ],
    };

    assert_eq!(
        struct_tag1.abstract_size_for_gas_metering(),
        AbstractMemorySize::new(
            u64::from(*TYPETAG_ENUM_ABSTRACT_SIZE) * 15 + 10 + 10 + (AccountAddress::LENGTH as u64)
        )
    );

    let type_tag4 = TypeTag::Struct(Box::new(struct_tag1.clone()));

    assert_eq!(
        type_tag4.abstract_size_for_gas_metering(),
        struct_tag1.abstract_size_for_gas_metering() + *TYPETAG_ENUM_ABSTRACT_SIZE
    );
}

#[test]
fn test_type_tag_deserialize_case_insensitive() {
    let org_struct_tag = StructTag {
        address: AccountAddress::ONE,
        module: Identifier::from(IdentStr::new("TestModule").unwrap()),
        name: Identifier::from(IdentStr::new("TestStruct").unwrap()),
        type_params: vec![
            TypeTag::U8,
            TypeTag::U16,
            TypeTag::U32,
            TypeTag::U64,
            TypeTag::U128,
            TypeTag::U256,
            TypeTag::Bool,
            TypeTag::Address,
            TypeTag::Signer,
        ],
    };

    let current_json = serde_json::to_string(&org_struct_tag).unwrap();

    let upper_case_json = format!(
        r##"{{"address":"{}","module":"TestModule","name":"TestStruct","type_params":["U8","U16","U32","U64","U128","U256","Bool","Address","Signer"]}}"##,
        AccountAddress::ONE
    );
    let upper_case_decoded = serde_json::from_str(upper_case_json.as_str()).unwrap();
    assert_eq!(org_struct_tag, upper_case_decoded);

    let lower_case_json = format!(
        r##"{{"address":"{}","module":"TestModule","name":"TestStruct","type_args":["u8","u16","u32","u64","u128","u256","bool","address","signer"]}}"##,
        AccountAddress::ONE
    );
    let lower_case_decoded = serde_json::from_str(lower_case_json.as_str()).unwrap();
    assert_eq!(org_struct_tag, lower_case_decoded);

    assert_eq!(current_json, lower_case_json);
}
