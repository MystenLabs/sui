// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

extern crate test_generation;
use move_binary_format::file_format::{
    empty_module, Ability, AbilitySet, Bytecode, CompiledModule, DatatypeHandle,
    DatatypeHandleIndex, FieldDefinition, FieldHandle, FieldHandleIndex, IdentifierIndex,
    ModuleHandleIndex, SignatureToken, StructDefinition, StructDefinitionIndex,
    StructFieldInformation, TableIndex, TypeSignature,
};
use move_core_types::identifier::Identifier;
use std::collections::HashMap;
use test_generation::{
    abilities,
    abstract_state::{AbstractState, AbstractValue, CallGraph},
};

mod common;

fn generate_module_with_struct(resource: bool) -> CompiledModule {
    let mut module: CompiledModule = empty_module();

    let struct_index = 0;
    let num_fields = 5;
    let offset = module.identifiers.len() as TableIndex;
    module.identifiers.push(Identifier::new("struct0").unwrap());

    let mut fields = vec![];
    for i in 0..num_fields {
        module
            .identifiers
            .push(Identifier::new(format!("string{}", i)).unwrap());
        let str_pool_idx = IdentifierIndex::new(i + 1);
        fields.push(FieldDefinition {
            name: str_pool_idx,
            signature: TypeSignature(SignatureToken::Bool),
        });
    }
    let struct_def = StructDefinition {
        struct_handle: DatatypeHandleIndex(struct_index),
        field_information: StructFieldInformation::Declared(fields),
    };
    module.struct_defs.push(struct_def);
    module.datatype_handles = vec![DatatypeHandle {
        module: ModuleHandleIndex::new(0),
        name: IdentifierIndex::new((struct_index + offset) as TableIndex),
        abilities: if resource {
            AbilitySet::EMPTY | Ability::Key | Ability::Store
        } else {
            AbilitySet::PRIMITIVES
        },
        type_parameters: vec![],
    }];
    module
}

fn create_struct_value(module: &CompiledModule) -> (AbstractValue, Vec<SignatureToken>) {
    let struct_def = module.struct_def_at(StructDefinitionIndex::new(0));
    let tokens: Vec<SignatureToken> = struct_def
        .fields()
        .into_iter()
        .flatten()
        .map(|field| field.signature.0.clone())
        .collect();
    let shandle = module.datatype_handle_at(struct_def.struct_handle);
    let struct_abilities = shandle.abilities;

    let type_argument_abilities = tokens.iter().map(|arg| abilities(module, arg, &[]));
    let declared_phantom_parameters = [false].repeat(type_argument_abilities.len());
    let abilities = AbilitySet::polymorphic_abilities(
        struct_abilities,
        declared_phantom_parameters,
        type_argument_abilities,
    )
    .unwrap();
    (
        AbstractValue::new_struct(
            SignatureToken::Datatype(struct_def.struct_handle),
            abilities,
        ),
        tokens,
    )
}

fn get_field_signature<'a>(module: &'a CompiledModule, handle: &FieldHandle) -> &'a SignatureToken {
    let struct_def = &module.struct_defs[handle.owner.0 as usize];
    match &struct_def.field_information {
        StructFieldInformation::Native => panic!("borrow field on a native struct"),
        StructFieldInformation::Declared(fields) => &fields[handle.field as usize].signature.0,
    }
}

#[test]
#[should_panic]
fn bytecode_pack_signature_not_satisfied() {
    let module = generate_module_with_struct(false);
    let state1 =
        AbstractState::from_locals(module, HashMap::new(), vec![], vec![], CallGraph::new(0));
    common::run_instruction(Bytecode::Pack(StructDefinitionIndex::new(0)), state1);
}

#[test]
fn bytecode_pack() {
    let module = generate_module_with_struct(false);
    let mut state1 =
        AbstractState::from_locals(module, HashMap::new(), vec![], vec![], CallGraph::new(0));
    let (struct_value1, tokens) = create_struct_value(&state1.module.module);
    for token in tokens {
        let abstract_value = AbstractValue {
            token: token.clone(),
            abilities: abilities(&state1.module.module, &token, &[]),
        };
        state1.stack_push(abstract_value);
    }
    let (state2, _) =
        common::run_instruction(Bytecode::Pack(StructDefinitionIndex::new(0)), state1);
    let struct_value2 = state2.stack_peek(0).expect("struct not added to stack");
    assert_eq!(
        struct_value1, struct_value2,
        "stack type postcondition not met"
    );
}

#[test]
#[should_panic]
fn bytecode_unpack_signature_not_satisfied() {
    let module = generate_module_with_struct(false);
    let state1 =
        AbstractState::from_locals(module, HashMap::new(), vec![], vec![], CallGraph::new(0));
    common::run_instruction(Bytecode::Unpack(StructDefinitionIndex::new(0)), state1);
}

#[test]
fn bytecode_unpack() {
    let module = generate_module_with_struct(false);
    let mut state1 =
        AbstractState::from_locals(module, HashMap::new(), vec![], vec![], CallGraph::new(0));
    let (struct_value, tokens) = create_struct_value(&state1.module.module);
    state1.stack_push(struct_value);
    let (state2, _) =
        common::run_instruction(Bytecode::Unpack(StructDefinitionIndex::new(0)), state1);
    assert_eq!(
        state2.stack_len(),
        tokens.len(),
        "stack type postcondition not met"
    );
}

#[test]
fn bytecode_mutborrowfield() {
    let mut module: CompiledModule = generate_module_with_struct(false);
    let struct_def_idx = StructDefinitionIndex((module.struct_defs.len() - 1) as u16);
    module.field_handles.push(FieldHandle {
        owner: struct_def_idx,
        field: 0,
    });
    let field_handle_idx = FieldHandleIndex((module.field_handles.len() - 1) as u16);
    let field_signature =
        get_field_signature(&module, &module.field_handles[field_handle_idx.0 as usize]).clone();

    let mut state1 =
        AbstractState::from_locals(module, HashMap::new(), vec![], vec![], CallGraph::new(0));
    let struct_value = create_struct_value(&state1.module.module).0;
    state1.stack_push(AbstractValue {
        token: SignatureToken::MutableReference(Box::new(struct_value.token)),
        abilities: struct_value.abilities,
    });
    let (state2, _) = common::run_instruction(Bytecode::MutBorrowField(field_handle_idx), state1);
    let abilities = abilities(&state2.module.module, &field_signature, &[]);
    assert_eq!(
        state2.stack_peek(0),
        Some(AbstractValue {
            token: SignatureToken::MutableReference(Box::new(field_signature)),
            abilities,
        }),
        "stack type postcondition not met"
    );
}

#[test]
#[should_panic]
fn bytecode_mutborrowfield_stack_has_no_reference() {
    let mut module: CompiledModule = generate_module_with_struct(false);
    let struct_def_idx = StructDefinitionIndex((module.struct_defs.len() - 1) as u16);
    module.field_handles.push(FieldHandle {
        owner: struct_def_idx,
        field: 0,
    });
    let field_handle_idx = FieldHandleIndex((module.field_handles.len() - 1) as u16);

    let state1 =
        AbstractState::from_locals(module, HashMap::new(), vec![], vec![], CallGraph::new(0));
    common::run_instruction(Bytecode::MutBorrowField(field_handle_idx), state1);
}

#[test]
#[should_panic]
fn bytecode_mutborrowfield_ref_is_immutable() {
    let mut module: CompiledModule = generate_module_with_struct(false);
    let struct_def_idx = StructDefinitionIndex((module.struct_defs.len() - 1) as u16);
    module.field_handles.push(FieldHandle {
        owner: struct_def_idx,
        field: 0,
    });
    let field_handle_idx = FieldHandleIndex((module.field_handles.len() - 1) as u16);

    let mut state1 =
        AbstractState::from_locals(module, HashMap::new(), vec![], vec![], CallGraph::new(0));
    let struct_value = create_struct_value(&state1.module.module).0;
    state1.stack_push(AbstractValue {
        token: SignatureToken::Reference(Box::new(struct_value.token)),
        abilities: struct_value.abilities,
    });
    common::run_instruction(Bytecode::MutBorrowField(field_handle_idx), state1);
}

#[test]
fn bytecode_immborrowfield() {
    let mut module: CompiledModule = generate_module_with_struct(false);
    let struct_def_idx = StructDefinitionIndex((module.struct_defs.len() - 1) as u16);
    module.field_handles.push(FieldHandle {
        owner: struct_def_idx,
        field: 0,
    });
    let field_handle_idx = FieldHandleIndex((module.field_handles.len() - 1) as u16);
    let field_signature =
        get_field_signature(&module, &module.field_handles[field_handle_idx.0 as usize]).clone();

    let mut state1 =
        AbstractState::from_locals(module, HashMap::new(), vec![], vec![], CallGraph::new(0));
    let struct_value = create_struct_value(&state1.module.module).0;
    state1.stack_push(AbstractValue {
        token: SignatureToken::Reference(Box::new(struct_value.token)),
        abilities: struct_value.abilities,
    });
    let (state2, _) = common::run_instruction(Bytecode::ImmBorrowField(field_handle_idx), state1);
    let abilities = abilities(&state2.module.module, &field_signature, &[]);
    assert_eq!(
        state2.stack_peek(0),
        Some(AbstractValue {
            token: SignatureToken::MutableReference(Box::new(field_signature)),
            abilities,
        }),
        "stack type postcondition not met"
    );
}

#[test]
#[should_panic]
fn bytecode_immborrowfield_stack_has_no_reference() {
    let mut module: CompiledModule = generate_module_with_struct(false);
    let struct_def_idx = StructDefinitionIndex((module.struct_defs.len() - 1) as u16);
    module.field_handles.push(FieldHandle {
        owner: struct_def_idx,
        field: 0,
    });
    let field_handle_idx = FieldHandleIndex((module.field_handles.len() - 1) as u16);

    let state1 =
        AbstractState::from_locals(module, HashMap::new(), vec![], vec![], CallGraph::new(0));
    common::run_instruction(Bytecode::ImmBorrowField(field_handle_idx), state1);
}

#[test]
#[should_panic]
fn bytecode_immborrowfield_ref_is_mutable() {
    let mut module: CompiledModule = generate_module_with_struct(false);
    let struct_def_idx = StructDefinitionIndex((module.struct_defs.len() - 1) as u16);
    module.field_handles.push(FieldHandle {
        owner: struct_def_idx,
        field: 0,
    });
    let field_handle_idx = FieldHandleIndex((module.field_handles.len() - 1) as u16);

    let mut state1 =
        AbstractState::from_locals(module, HashMap::new(), vec![], vec![], CallGraph::new(0));
    let struct_value = create_struct_value(&state1.module.module).0;
    state1.stack_push(AbstractValue {
        token: SignatureToken::MutableReference(Box::new(struct_value.token)),
        abilities: struct_value.abilities,
    });
    common::run_instruction(Bytecode::ImmBorrowField(field_handle_idx), state1);
}
