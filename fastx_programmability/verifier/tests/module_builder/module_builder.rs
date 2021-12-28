// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use fastx_types::FASTX_FRAMEWORK_ADDRESS;
use move_binary_format::file_format::*;
use move_core_types::{account_address::AccountAddress, identifier::Identifier};

pub struct ModuleBuilder {
    module: CompiledModule,
}

impl ModuleBuilder {
    pub fn new(address: AccountAddress, name: &str) -> Self {
        Self {
            module: CompiledModule {
                version: move_binary_format::file_format_common::VERSION_MAX,
                module_handles: vec![ModuleHandle {
                    address: AddressIdentifierIndex(0),
                    name: IdentifierIndex(0),
                }],
                self_module_handle_idx: ModuleHandleIndex(0),
                identifiers: vec![Identifier::new(name).unwrap()],
                address_identifiers: vec![address],
                struct_handles: vec![],
                struct_defs: vec![],
                function_handles: vec![],
                function_defs: vec![],
                signatures: vec![
                    Signature(vec![]), // void
                ],
                constant_pool: vec![],
                field_handles: vec![],
                friend_decls: vec![],
                struct_def_instantiations: vec![],
                function_instantiations: vec![],
                field_instantiations: vec![],
            },
        }
    }

    pub fn default() -> Self {
        Self::new(FASTX_FRAMEWORK_ADDRESS, "ID")
    }

    pub fn get_module(&self) -> &CompiledModule {
        &self.module
    }

    pub fn get_self_index(&self) -> ModuleHandleIndex {
        self.module.self_module_handle_idx
    }

    pub fn add_function(
        &mut self,
        module_idx: ModuleHandleIndex,
        name: &str,
        parameters: Vec<SignatureToken>,
        ret: Vec<SignatureToken>,
    ) -> (FunctionHandleIndex, FunctionDefinitionIndex) {
        let new_handle = FunctionHandle {
            module: module_idx,
            name: self.add_identifier(name),
            parameters: self.add_signature(parameters),
            return_: self.add_signature(ret),
            type_parameters: vec![],
        };
        let handle_idx = FunctionHandleIndex(self.module.function_handles.len() as u16);
        self.module.function_handles.push(new_handle);
        let new_def = FunctionDefinition {
            function: handle_idx,
            visibility: Visibility::Public,
            acquires_global_resources: vec![],
            code: Some(CodeUnit {
                locals: SignatureIndex(0),
                code: vec![Bytecode::Ret],
            }),
        };
        self.module.function_defs.push(new_def);
        (
            handle_idx,
            FunctionDefinitionIndex((self.module.function_defs.len() - 1) as u16),
        )
    }

    pub fn add_struct(
        &mut self,
        module_index: ModuleHandleIndex,
        name: &str,
        abilities: AbilitySet,
        fields: Vec<FieldDefinition>,
    ) -> (StructHandleIndex, StructDefinitionIndex) {
        let new_handle = StructHandle {
            module: module_index,
            name: self.add_identifier(name),
            abilities,
            type_parameters: vec![],
        };
        let handle_idx = StructHandleIndex(self.module.struct_handles.len() as u16);
        self.module.struct_handles.push(new_handle);
        let new_def = StructDefinition {
            struct_handle: handle_idx,
            field_information: StructFieldInformation::Declared(fields),
        };
        self.module.struct_defs.push(new_def);
        (
            handle_idx,
            StructDefinitionIndex((self.module.struct_defs.len() - 1) as u16),
        )
    }

    pub fn add_module(&mut self, address: AccountAddress, name: &str) -> ModuleHandleIndex {
        let handle = ModuleHandle {
            address: self.add_address(address),
            name: self.add_identifier(name),
        };
        self.module.module_handles.push(handle);
        ModuleHandleIndex((self.module.module_handles.len() - 1) as u16)
    }

    pub fn create_field(&mut self, name: &str, ty: SignatureToken) -> FieldDefinition {
        let id = self.add_identifier(name);
        FieldDefinition {
            name: id,
            signature: TypeSignature(ty),
        }
    }

    pub fn set_bytecode(&mut self, func_def: FunctionDefinitionIndex, bytecode: Vec<Bytecode>) {
        let code = &mut self.module.function_defs[func_def.0 as usize]
            .code
            .as_mut()
            .unwrap()
            .code;
        *code = bytecode;
    }

    fn add_identifier(&mut self, id: &str) -> IdentifierIndex {
        self.module.identifiers.push(Identifier::new(id).unwrap());
        IdentifierIndex((self.module.identifiers.len() - 1) as u16)
    }

    fn add_signature(&mut self, sig: Vec<SignatureToken>) -> SignatureIndex {
        self.module.signatures.push(Signature(sig));
        SignatureIndex((self.module.signatures.len() - 1) as u16)
    }

    fn add_address(&mut self, address: AccountAddress) -> AddressIdentifierIndex {
        self.module.address_identifiers.push(address);
        AddressIdentifierIndex((self.module.address_identifiers.len() - 1) as u16)
    }
}
