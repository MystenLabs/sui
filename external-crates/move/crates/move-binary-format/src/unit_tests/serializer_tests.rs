// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    file_format::{
        basic_test_module, basic_test_module_with_enum, Bytecode, CodeUnit,
        EnumDefInstantiationIndex, EnumDefinitionIndex, SignatureIndex, VariantJumpTable,
        VariantJumpTableIndex,
    },
    file_format_common::*,
    CompiledModule,
};

#[test]
fn enum_serialize_version_invalid() {
    // With enums and an invalid version bytecode version
    let module = basic_test_module_with_enum();
    let mut v = vec![];
    assert!(module
        .serialize_for_version(Some(VERSION_6), &mut v)
        .is_err());

    for instruction in [
        Bytecode::PackVariant(EnumDefinitionIndex(0), 0),
        Bytecode::UnpackVariant(EnumDefinitionIndex(0), 0),
        Bytecode::UnpackVariantImmRef(EnumDefinitionIndex(0), 0),
        Bytecode::UnpackVariantMutRef(EnumDefinitionIndex(0), 0),
        Bytecode::UnpackVariantGeneric(EnumDefInstantiationIndex(0), 0),
        Bytecode::UnpackVariantGenericImmRef(EnumDefInstantiationIndex(0), 0),
        Bytecode::UnpackVariantGenericMutRef(EnumDefInstantiationIndex(0), 0),
        Bytecode::VariantSwitch(VariantJumpTableIndex::new(0)),
    ] {
        let mut module = basic_test_module();
        module.function_defs[0].code = Some(CodeUnit {
            locals: SignatureIndex::new(0),
            code: vec![instruction, Bytecode::Ret],
            jump_tables: vec![],
        });
        let mut v = vec![];
        assert!(module
            .serialize_for_version(Some(VERSION_6), &mut v)
            .is_err());
        // Serialization version does not take into account the bytecode version that it is
        // specified in the module, only the version that is supplied to the
        // `serialize_for_version` function.
        module.version = VERSION_6;
        assert!(module
            .serialize_for_version(Some(VERSION_MAX), &mut v)
            .is_ok());
    }

    let mut module = basic_test_module();
    module.function_defs[0].code = Some(CodeUnit {
        locals: SignatureIndex::new(0),
        code: vec![Bytecode::Ret],
        jump_tables: vec![VariantJumpTable {
            head_enum: EnumDefinitionIndex(0),
            jump_table: vec![],
        }],
    });
    let mut v = vec![];
    assert!(module
        .serialize_for_version(Some(VERSION_6), &mut v)
        .is_err());

    // Module without enums can still be serialized at version 6
    let module = basic_test_module();
    let mut v = vec![];
    assert!(module
        .serialize_for_version(Some(VERSION_6), &mut v)
        .is_ok());

    // Can be deserialized at version 6 and at max version as well.
    CompiledModule::deserialize_with_config(
        &v,
        VERSION_MAX,
        /*check_no_extraneous_bytes*/ true,
    )
    .unwrap();

    CompiledModule::deserialize_with_config(&v, VERSION_6, /*check_no_extraneous_bytes*/ true)
        .unwrap();
}

#[test]
fn versions_serialization_round_trip() {
    for version in [VERSION_6, VERSION_MAX] {
        let mut module = basic_test_module();
        module.version = version;
        let mut v = vec![];
        assert!(module
            .serialize_for_version(Some(VERSION_6), &mut v)
            .is_ok());

        // Can deserialize at version 6
        let module6 = CompiledModule::deserialize_with_config(
            &v, VERSION_6, /*check_no_extraneous_bytes*/ true,
        )
        .unwrap();

        // Can deserialize at version max
        let module7 = CompiledModule::deserialize_with_config(
            &v,
            VERSION_MAX,
            /*check_no_extraneous_bytes*/ true,
        )
        .unwrap();

        // module6 can be serialized at versions 6 & 7
        let mut v = vec![];
        assert!(module6
            .serialize_for_version(Some(VERSION_6), &mut v)
            .is_ok());
        let mut v = vec![];
        assert!(module6
            .serialize_for_version(Some(VERSION_MAX), &mut v)
            .is_ok());
        // module7 can be serialized at version 7 and version 6 because it doesn't have
        // enum/enum-related instructions.
        let mut v = vec![];
        assert!(module7
            .serialize_for_version(Some(VERSION_6), &mut v)
            .is_ok());
        let mut v = vec![];
        assert!(module7
            .serialize_for_version(Some(VERSION_MAX), &mut v)
            .is_ok());
    }
}

// Test that the serialization version is upgraded to the version specified during serialization
// and not the version specified in the module if an override is provided.
#[test]
fn serialization_upgrades_version() {
    let mut module = basic_test_module();
    let mut m_bytes = vec![];
    module.version = VERSION_6;
    assert!(module
        .serialize_for_version(Some(VERSION_MAX), &mut m_bytes)
        .is_ok());
    let v_max_bytes = VERSION_MAX.to_le_bytes();
    let v_6_bytes = VERSION_6.to_le_bytes();
    assert_eq!(
        m_bytes[BinaryConstants::MOVE_MAGIC_SIZE
            ..BinaryConstants::MOVE_MAGIC_SIZE + v_max_bytes.len()],
        v_max_bytes
    );
    assert_ne!(
        m_bytes
            [BinaryConstants::MOVE_MAGIC_SIZE..BinaryConstants::MOVE_MAGIC_SIZE + v_6_bytes.len()],
        v_6_bytes
    );
}

// Test that the serialization version matches the module's version if no override is provided.
#[test]
fn serialization_upgrades_version_no_override() {
    let mut module = basic_test_module();
    let mut m_bytes = vec![];
    module.version = VERSION_6;
    assert!(module.serialize(&mut m_bytes).is_ok());
    let v_max_bytes = VERSION_MAX.to_le_bytes();
    let v_6_bytes = VERSION_6.to_le_bytes();
    assert_ne!(
        m_bytes[BinaryConstants::MOVE_MAGIC_SIZE
            ..BinaryConstants::MOVE_MAGIC_SIZE + v_max_bytes.len()],
        v_max_bytes
    );
    assert_eq!(
        m_bytes
            [BinaryConstants::MOVE_MAGIC_SIZE..BinaryConstants::MOVE_MAGIC_SIZE + v_6_bytes.len()],
        v_6_bytes
    );
}
