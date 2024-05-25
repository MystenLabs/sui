// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    binary_config::{BinaryConfig, TableConfig},
    file_format::{
        basic_test_module, basic_test_module_with_enum, Bytecode, CodeUnit, EnumDefInstantiation,
        EnumDefInstantiationIndex, EnumDefinitionIndex, JumpTableInner, SignatureIndex,
        VariantHandle, VariantHandleIndex, VariantInstantiationHandle,
        VariantInstantiationHandleIndex, VariantJumpTable, VariantJumpTableIndex,
    },
    file_format_common::*,
    CompiledModule,
};

#[test]
fn enum_serialize_version_invalid() {
    // With enums and an invalid version bytecode version
    let module = basic_test_module_with_enum();
    let mut v = vec![];
    assert!(module.serialize_with_version(VERSION_6, &mut v).is_err());

    for instruction in [
        Bytecode::PackVariant(VariantHandleIndex(0)),
        Bytecode::UnpackVariant(VariantHandleIndex(0)),
        Bytecode::UnpackVariantImmRef(VariantHandleIndex(0)),
        Bytecode::UnpackVariantMutRef(VariantHandleIndex(0)),
        Bytecode::UnpackVariantGeneric(VariantInstantiationHandleIndex(0)),
        Bytecode::UnpackVariantGenericImmRef(VariantInstantiationHandleIndex(0)),
        Bytecode::UnpackVariantGenericMutRef(VariantInstantiationHandleIndex(0)),
        Bytecode::VariantSwitch(VariantJumpTableIndex::new(0)),
    ] {
        let mut module = basic_test_module();
        module.function_defs[0].code = Some(CodeUnit {
            locals: SignatureIndex::new(0),
            code: vec![instruction, Bytecode::Ret],
            jump_tables: vec![],
        });
        let mut v = vec![];
        assert!(module.serialize_with_version(VERSION_6, &mut v).is_err());
        // Serialization version does not take into account the bytecode version that it is
        // specified in the module, only the version that is supplied to the
        // `serialize_with_version` function.
        module.version = VERSION_6;
        assert!(module.serialize_with_version(VERSION_MAX, &mut v).is_ok());
    }

    let mut module = basic_test_module();
    module.function_defs[0].code = Some(CodeUnit {
        locals: SignatureIndex::new(0),
        code: vec![Bytecode::Ret],
        jump_tables: vec![VariantJumpTable {
            head_enum: EnumDefinitionIndex(0),
            jump_table: JumpTableInner::Full(vec![]),
        }],
    });
    let mut v = vec![];
    assert!(module.serialize_with_version(VERSION_6, &mut v).is_err());

    // Module without enums can still be serialized at version 6
    let module = basic_test_module();
    let mut v = vec![];
    assert!(module.serialize_with_version(VERSION_6, &mut v).is_ok());

    // Can be deserialized at version 6 and at max version as well.
    CompiledModule::deserialize_with_config(&v, &BinaryConfig::with_extraneous_bytes_check(true))
        .unwrap();

    CompiledModule::deserialize_with_config(
        &v,
        &BinaryConfig::new(VERSION_6, VERSION_6, true, TableConfig::legacy()),
    )
    .unwrap();
}

#[test]
fn enum_serialize_variant_handle_wrong_version() {
    // With enums and an invalid version bytecode version
    let mut module = basic_test_module();
    let mut v = vec![];
    // Invalid variant handle
    module.variant_handles.push(VariantHandle {
        enum_def: EnumDefinitionIndex(0),
        variant: 0,
    });
    assert!(module.serialize_with_version(VERSION_6, &mut v).is_err());
}

#[test]
fn enum_serialize_variant_handle_instantiation_wrong_version() {
    // With enums and an invalid version bytecode version
    let mut module = basic_test_module();
    let mut v = vec![];
    // Invalid variant handle
    module
        .variant_instantiation_handles
        .push(VariantInstantiationHandle {
            enum_def: EnumDefInstantiationIndex(0),
            variant: 0,
        });
    assert!(module.serialize_with_version(VERSION_6, &mut v).is_err());
}

#[test]
fn enum_serialize_enum_def_instantiation_wrong_version() {
    // With enums and an invalid version bytecode version
    let mut module = basic_test_module();
    let mut v = vec![];
    // Invalid variant handle
    module.enum_def_instantiations.push(EnumDefInstantiation {
        def: EnumDefinitionIndex(0),
        type_parameters: SignatureIndex(0),
    });
    assert!(module.serialize_with_version(VERSION_6, &mut v).is_err());
}

#[test]
fn versions_serialization_round_trip() {
    for version in [VERSION_6, VERSION_MAX] {
        let mut module = basic_test_module();
        module.version = version;
        let mut v = vec![];
        assert!(module.serialize_with_version(VERSION_6, &mut v).is_ok());

        // Can deserialize at version 6
        let module6 = CompiledModule::deserialize_with_config(
            &v,
            &BinaryConfig::new(VERSION_6, VERSION_6, true, TableConfig::legacy()),
        )
        .unwrap();

        // Can deserialize at version max
        let module7 = CompiledModule::deserialize_with_config(
            &v,
            &BinaryConfig::new(VERSION_MAX, VERSION_6, true, TableConfig::legacy()),
        )
        .unwrap();

        // module6 can be serialized at versions 6 & 7
        let mut v = vec![];
        assert!(module6.serialize_with_version(VERSION_6, &mut v).is_ok());
        let mut v = vec![];
        assert!(module6.serialize_with_version(VERSION_MAX, &mut v).is_ok());
        // module7 can be serialized at version 7 and version 6 because it doesn't have
        // enum/enum-related instructions.
        let mut v = vec![];
        assert!(module7.serialize_with_version(VERSION_6, &mut v).is_ok());
        let mut v = vec![];
        assert!(module7.serialize_with_version(VERSION_MAX, &mut v).is_ok());
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
        .serialize_with_version(VERSION_MAX, &mut m_bytes)
        .is_ok());
    let v_max_bytes = BinaryFlavor::encode_version(VERSION_MAX).to_le_bytes();
    let v_6_bytes = BinaryFlavor::encode_version(VERSION_6).to_le_bytes();
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

#[test]
fn serialization_upgrades_version_no_override() {
    let module = basic_test_module();
    let mut m_bytes = vec![];
    assert!(module
        .serialize_with_version(module.version, &mut m_bytes)
        .is_ok());
    let v_max_bytes = BinaryFlavor::encode_version(VERSION_MAX).to_le_bytes();
    assert_eq!(
        m_bytes[BinaryConstants::MOVE_MAGIC_SIZE
            ..BinaryConstants::MOVE_MAGIC_SIZE + v_max_bytes.len()],
        v_max_bytes
    );
}

#[test]
fn serialize_v6_has_no_flavor() {
    let module = basic_test_module();
    let mut bin = vec![];
    module.serialize_with_version(VERSION_6, &mut bin).unwrap();
    let v6_bytes = VERSION_6.to_le_bytes();
    let v6_flavor_bytes = BinaryFlavor::encode_version(VERSION_6).to_le_bytes();
    // assert that no flavoring is added to v6
    assert_eq!(v6_bytes, v6_flavor_bytes);
    assert_eq!(
        bin[BinaryConstants::MOVE_MAGIC_SIZE..BinaryConstants::MOVE_MAGIC_SIZE + v6_bytes.len()],
        v6_bytes
    );
}

#[test]
fn serialize_v7_has_flavor() {
    let module = basic_test_module();
    let mut bin = vec![];
    module.serialize_with_version(VERSION_7, &mut bin).unwrap();
    let v7_flavored_bytes = BinaryFlavor::encode_version(VERSION_7).to_le_bytes();
    let v7_unflavored_bytes = VERSION_7.to_le_bytes();
    assert_eq!(
        bin[BinaryConstants::MOVE_MAGIC_SIZE
            ..BinaryConstants::MOVE_MAGIC_SIZE + v7_flavored_bytes.len()],
        v7_flavored_bytes
    );
    assert_ne!(
        bin[BinaryConstants::MOVE_MAGIC_SIZE
            ..BinaryConstants::MOVE_MAGIC_SIZE + v7_unflavored_bytes.len()],
        v7_unflavored_bytes
    );
}
