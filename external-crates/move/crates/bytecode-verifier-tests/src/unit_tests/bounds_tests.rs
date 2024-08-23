// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::{check_bounds::BoundsChecker, file_format::*, file_format_common};
use move_core_types::vm_status::StatusCode;

#[test]
fn empty_module_no_errors() {
    BoundsChecker::verify_module(&basic_test_module()).unwrap();
}

#[test]
fn invalid_default_module() {
    BoundsChecker::verify_module(&CompiledModule {
        version: file_format_common::VERSION_MAX,
        ..Default::default()
    })
    .unwrap_err();
}

#[test]
fn invalid_self_module_handle_index() {
    let mut m = basic_test_module();
    m.self_module_handle_idx = ModuleHandleIndex(12);
    assert_eq!(
        BoundsChecker::verify_module(&m).unwrap_err().major_status(),
        StatusCode::INDEX_OUT_OF_BOUNDS
    );
}

#[test]
fn invalid_type_param_in_fn_return_() {
    use SignatureToken::*;

    let mut m = basic_test_module();
    m.function_handles[0].return_ = SignatureIndex(1);
    m.signatures.push(Signature(vec![TypeParameter(0)]));
    assert_eq!(m.signatures.len(), 2);
    assert_eq!(
        BoundsChecker::verify_module(&m).unwrap_err().major_status(),
        StatusCode::INDEX_OUT_OF_BOUNDS
    );
}

#[test]
fn invalid_type_param_in_fn_parameters() {
    use SignatureToken::*;

    let mut m = basic_test_module();
    m.function_handles[0].parameters = SignatureIndex(1);
    m.signatures.push(Signature(vec![TypeParameter(0)]));
    assert_eq!(
        BoundsChecker::verify_module(&m).unwrap_err().major_status(),
        StatusCode::INDEX_OUT_OF_BOUNDS
    );
}

#[test]
fn invalid_struct_in_fn_return_() {
    use SignatureToken::*;

    let mut m = basic_test_module();
    m.function_handles[0].return_ = SignatureIndex(1);
    m.signatures
        .push(Signature(vec![Datatype(DatatypeHandleIndex::new(1))]));
    assert_eq!(
        BoundsChecker::verify_module(&m).unwrap_err().major_status(),
        StatusCode::INDEX_OUT_OF_BOUNDS
    );
}

#[test]
fn invalid_type_param_in_field() {
    use SignatureToken::*;

    let mut m = basic_test_module();
    match &mut m.struct_defs[0].field_information {
        StructFieldInformation::Declared(ref mut fields) => {
            fields[0].signature.0 = TypeParameter(0);
            assert_eq!(
                BoundsChecker::verify_module(&m).unwrap_err().major_status(),
                StatusCode::INDEX_OUT_OF_BOUNDS
            );
        }
        _ => panic!("attempt to change a field that does not exist"),
    }
}

#[test]
fn invalid_struct_in_field() {
    use SignatureToken::*;

    let mut m = basic_test_module();
    match &mut m.struct_defs[0].field_information {
        StructFieldInformation::Declared(ref mut fields) => {
            fields[0].signature.0 = Datatype(DatatypeHandleIndex::new(3));
            assert_eq!(
                BoundsChecker::verify_module(&m).unwrap_err().major_status(),
                StatusCode::INDEX_OUT_OF_BOUNDS
            );
        }
        _ => panic!("attempt to change a field that does not exist"),
    }
}

#[test]
fn invalid_struct_with_actuals_in_field() {
    use SignatureToken::*;

    let mut m = basic_test_module();
    match &mut m.struct_defs[0].field_information {
        StructFieldInformation::Declared(ref mut fields) => {
            fields[0].signature.0 = DatatypeInstantiation(Box::new((
                DatatypeHandleIndex::new(0),
                vec![TypeParameter(0)],
            )));
            assert_eq!(
                BoundsChecker::verify_module(&m).unwrap_err().major_status(),
                StatusCode::NUMBER_OF_TYPE_ARGUMENTS_MISMATCH
            );
        }
        _ => panic!("attempt to change a field that does not exist"),
    }
}

#[test]
fn invalid_locals_id_in_call() {
    use Bytecode::*;

    let mut m = basic_test_module();
    m.function_instantiations.push(FunctionInstantiation {
        handle: FunctionHandleIndex::new(0),
        type_parameters: SignatureIndex::new(1),
    });
    let func_inst_idx = FunctionInstantiationIndex(m.function_instantiations.len() as u16 - 1);
    m.function_defs[0].code.as_mut().unwrap().code = vec![CallGeneric(func_inst_idx)];
    assert_eq!(
        BoundsChecker::verify_module(&m).unwrap_err().major_status(),
        StatusCode::INDEX_OUT_OF_BOUNDS
    );
}

#[test]
fn invalid_type_param_in_call() {
    use Bytecode::*;
    use SignatureToken::*;

    let mut m = basic_test_module();
    m.signatures.push(Signature(vec![TypeParameter(0)]));
    m.function_instantiations.push(FunctionInstantiation {
        handle: FunctionHandleIndex::new(0),
        type_parameters: SignatureIndex::new(1),
    });
    let func_inst_idx = FunctionInstantiationIndex(m.function_instantiations.len() as u16 - 1);
    m.function_defs[0].code.as_mut().unwrap().code = vec![CallGeneric(func_inst_idx)];
    assert_eq!(
        BoundsChecker::verify_module(&m).unwrap_err().major_status(),
        StatusCode::INDEX_OUT_OF_BOUNDS
    );
}

#[test]
fn invalid_struct_as_type_actual_in_exists() {
    use Bytecode::*;
    use SignatureToken::*;

    let mut m = basic_test_module();
    m.signatures
        .push(Signature(vec![Datatype(DatatypeHandleIndex::new(3))]));
    m.function_instantiations.push(FunctionInstantiation {
        handle: FunctionHandleIndex::new(0),
        type_parameters: SignatureIndex::new(1),
    });
    let func_inst_idx = FunctionInstantiationIndex(m.function_instantiations.len() as u16 - 1);
    m.function_defs[0].code.as_mut().unwrap().code = vec![CallGeneric(func_inst_idx)];
    assert_eq!(
        BoundsChecker::verify_module(&m).unwrap_err().major_status(),
        StatusCode::INDEX_OUT_OF_BOUNDS
    );
}

#[test]
fn invalid_friend_module_address() {
    let mut m = basic_test_module();
    m.friend_decls.push(ModuleHandle {
        address: AddressIdentifierIndex::new(m.address_identifiers.len() as TableIndex),
        name: IdentifierIndex::new(0),
    });
    assert_eq!(
        BoundsChecker::verify_module(&m).unwrap_err().major_status(),
        StatusCode::INDEX_OUT_OF_BOUNDS
    );
}

#[test]
fn invalid_friend_module_name() {
    let mut m = basic_test_module();
    m.friend_decls.push(ModuleHandle {
        address: AddressIdentifierIndex::new(0),
        name: IdentifierIndex::new(m.identifiers.len() as TableIndex),
    });
    assert_eq!(
        BoundsChecker::verify_module(&m).unwrap_err().major_status(),
        StatusCode::INDEX_OUT_OF_BOUNDS
    );
}

#[test]
fn invalid_signature_for_vector_operation() {
    use Bytecode::*;

    let skeleton = basic_test_module();
    let sig_index = SignatureIndex(skeleton.signatures.len() as u16);
    for bytecode in [
        VecPack(sig_index, 0),
        VecLen(sig_index),
        VecImmBorrow(sig_index),
        VecMutBorrow(sig_index),
        VecPushBack(sig_index),
        VecPopBack(sig_index),
        VecUnpack(sig_index, 0),
        VecSwap(sig_index),
    ] {
        let mut m = skeleton.clone();
        m.function_defs[0].code.as_mut().unwrap().code = vec![bytecode];
        assert_eq!(
            BoundsChecker::verify_module(&m).unwrap_err().major_status(),
            StatusCode::INDEX_OUT_OF_BOUNDS
        );
    }
}

#[test]
fn invalid_struct_for_vector_operation() {
    use Bytecode::*;
    use SignatureToken::*;

    let mut skeleton = basic_test_module();
    skeleton
        .signatures
        .push(Signature(vec![Datatype(DatatypeHandleIndex::new(3))]));
    let sig_index = SignatureIndex((skeleton.signatures.len() - 1) as u16);
    for bytecode in [
        VecPack(sig_index, 0),
        VecLen(sig_index),
        VecImmBorrow(sig_index),
        VecMutBorrow(sig_index),
        VecPushBack(sig_index),
        VecPopBack(sig_index),
        VecUnpack(sig_index, 0),
        VecSwap(sig_index),
    ] {
        let mut m = skeleton.clone();
        m.function_defs[0].code.as_mut().unwrap().code = vec![bytecode];
        assert_eq!(
            BoundsChecker::verify_module(&m).unwrap_err().major_status(),
            StatusCode::INDEX_OUT_OF_BOUNDS
        );
    }
}

#[test]
fn invalid_type_param_for_vector_operation() {
    use Bytecode::*;
    use SignatureToken::*;

    let mut skeleton = basic_test_module();
    skeleton.signatures.push(Signature(vec![TypeParameter(0)]));
    let sig_index = SignatureIndex((skeleton.signatures.len() - 1) as u16);
    for bytecode in [
        VecPack(sig_index, 0),
        VecLen(sig_index),
        VecImmBorrow(sig_index),
        VecMutBorrow(sig_index),
        VecPushBack(sig_index),
        VecPopBack(sig_index),
        VecUnpack(sig_index, 0),
        VecSwap(sig_index),
    ] {
        let mut m = skeleton.clone();
        m.function_defs[0].code.as_mut().unwrap().code = vec![bytecode];
        assert_eq!(
            BoundsChecker::verify_module(&m).unwrap_err().major_status(),
            StatusCode::INDEX_OUT_OF_BOUNDS
        );
    }
}

#[test]
fn invalid_variant_handle_index_for_enum_operation() {
    use Bytecode::*;

    let skeleton = basic_test_module();
    let variant_handle_index = VariantHandleIndex(skeleton.variant_handles.len() as u16);
    let variant_handle_inst_index =
        VariantInstantiationHandleIndex(skeleton.variant_instantiation_handles.len() as u16);
    for bytecode in [
        PackVariant(variant_handle_index),
        UnpackVariant(variant_handle_index),
        UnpackVariantImmRef(variant_handle_index),
        UnpackVariantMutRef(variant_handle_index),
        PackVariantGeneric(variant_handle_inst_index),
        UnpackVariantGeneric(variant_handle_inst_index),
        UnpackVariantGenericImmRef(variant_handle_inst_index),
        UnpackVariantGenericMutRef(variant_handle_inst_index),
    ] {
        let mut m = skeleton.clone();
        m.function_defs[0].code.as_mut().unwrap().code = vec![bytecode];
        assert_eq!(
            BoundsChecker::verify_module(&m).unwrap_err().major_status(),
            StatusCode::INDEX_OUT_OF_BOUNDS
        );
    }
}

#[test]
fn invalid_variant_jump_table_index() {
    use Bytecode::*;

    let skeleton = basic_test_module();
    let jt_index = VariantJumpTableIndex(
        skeleton.function_defs[0]
            .code
            .as_ref()
            .map(|c| c.jump_tables.len() as u16)
            .unwrap_or(0u16),
    );
    let mut m = skeleton.clone();
    m.function_defs[0].code.as_mut().unwrap().code = vec![VariantSwitch(jt_index)];
    assert_eq!(
        BoundsChecker::verify_module(&m).unwrap_err().major_status(),
        StatusCode::INDEX_OUT_OF_BOUNDS
    );
}

#[test]
fn invalid_variant_jump_table_code_offset() {
    use Bytecode::*;

    let mut skeleton = basic_test_module_with_enum();
    let enum_index = EnumDefinitionIndex(0);
    skeleton.function_defs[0].code.as_mut().unwrap().code = vec![LdU64(0), Pop, Ret];
    skeleton.function_defs[0].code.as_mut().unwrap().jump_tables = vec![VariantJumpTable {
        head_enum: enum_index,
        jump_table: JumpTableInner::Full(vec![100]),
    }];

    let jt_index = VariantJumpTableIndex(
        skeleton.function_defs[0]
            .code
            .as_ref()
            .map(|c| c.jump_tables.len() as u16)
            .unwrap_or(0u16),
    );
    let mut m = skeleton.clone();
    m.function_defs[0].code.as_mut().unwrap().code = vec![VariantSwitch(jt_index)];
    assert_eq!(
        BoundsChecker::verify_module(&m).unwrap_err().major_status(),
        StatusCode::INDEX_OUT_OF_BOUNDS
    );
}
