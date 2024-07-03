// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::file_format::*;
use move_core_types::{account_address::AccountAddress, identifier::Identifier};
use sui_types::SUI_FRAMEWORK_ADDRESS;

pub struct ModuleBuilder {
    module: CompiledModule,
}

pub struct StructInfo {
    pub handle: DatatypeHandleIndex,
    pub def: StructDefinitionIndex,
    pub fields: Vec<FieldHandleIndex>,
}

pub struct FuncInfo {
    pub handle: FunctionHandleIndex,
    pub def: FunctionDefinitionIndex,
}

pub struct GenericFuncInfo {
    pub handle: FunctionInstantiationIndex,
    pub def: FunctionDefinitionIndex,
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

    /// Creates the "object" module in framework address, along with the "Info" struct.
    /// Both the module and the Info struct information are returned.
    pub fn default() -> (Self, StructInfo) {
        let mut module = Self::new(SUI_FRAMEWORK_ADDRESS, OBJECT_MODULE_NAME);
        let id = module.add_struct(
            module.get_self_index(),
            INFO_STRUCT_NAME,
            AbilitySet::EMPTY | Ability::Store | Ability::Drop,
            vec![],
        );
        (module, id)
    }

    pub fn get_module(&self) -> &CompiledModule {
        &self.module
    }

    pub fn get_self_index(&self) -> ModuleHandleIndex {
        self.module.self_module_handle_idx
    }

    pub fn add_function_verbose(
        &mut self,
        module_idx: ModuleHandleIndex,
        name: &str,
        parameters: Vec<SignatureToken>,
        ret: Vec<SignatureToken>,
        type_parameters: Vec<AbilitySet>,
        visibility: Visibility,
        code_unit: CodeUnit,
    ) -> FuncInfo {
        let new_handle = FunctionHandle {
            module: module_idx,
            name: self.add_identifier(name),
            parameters: self.add_signature(parameters),
            return_: self.add_signature(ret),
            type_parameters,
        };
        let handle_idx = FunctionHandleIndex(self.module.function_handles.len() as u16);
        self.module.function_handles.push(new_handle);
        let new_def = FunctionDefinition {
            function: handle_idx,
            visibility,
            acquires_global_resources: vec![],
            code: Some(code_unit),
        };
        self.module.function_defs.push(new_def);
        FuncInfo {
            handle: handle_idx,
            def: FunctionDefinitionIndex((self.module.function_defs.len() - 1) as u16),
        }
    }

    pub fn add_function(
        &mut self,
        module_idx: ModuleHandleIndex,
        name: &str,
        parameters: Vec<SignatureToken>,
        ret: Vec<SignatureToken>,
    ) -> FuncInfo {
        self.add_function_verbose(
            module_idx,
            name,
            parameters,
            ret,
            vec![],
            Visibility::Public,
            CodeUnit {
                locals: SignatureIndex(0),
                code: vec![Bytecode::Ret],
            },
        )
    }

    pub fn add_generic_function(
        &mut self,
        module_idx: ModuleHandleIndex,
        name: &str,
        type_parameters: Vec<SignatureToken>,
        parameters: Vec<SignatureToken>,
        ret: Vec<SignatureToken>,
    ) -> GenericFuncInfo {
        let func_info = self.add_function(module_idx, name, parameters, ret);
        let sig = self.add_signature(type_parameters);
        let handle_idx =
            FunctionInstantiationIndex(self.module.function_instantiations.len() as u16);
        self.module
            .function_instantiations
            .push(FunctionInstantiation {
                handle: func_info.handle,
                type_parameters: sig,
            });
        GenericFuncInfo {
            handle: handle_idx,
            def: func_info.def,
        }
    }

    pub fn add_struct_verbose(
        &mut self,
        module_index: ModuleHandleIndex,
        name: &str,
        abilities: AbilitySet,
        fields: Vec<(&str, SignatureToken)>,
        type_parameters: Vec<StructTypeParameter>,
    ) -> StructInfo {
        let new_handle = DatatypeHandle {
            module: module_index,
            name: self.add_identifier(name),
            abilities,
            type_parameters,
        };
        let handle_idx = DatatypeHandleIndex(self.module.struct_handles.len() as u16);
        self.module.struct_handles.push(new_handle);

        let field_len = fields.len();
        let field_defs = fields
            .into_iter()
            .map(|(name, ty)| self.create_field(name, ty))
            .collect();
        let new_def = StructDefinition {
            struct_handle: handle_idx,
            field_information: StructFieldInformation::Declared(field_defs),
        };
        let def_idx = StructDefinitionIndex(self.module.struct_defs.len() as u16);
        self.module.struct_defs.push(new_def);

        let field_handles = (0..field_len)
            .map(|idx| self.add_field_handle(def_idx, idx as u16))
            .collect();

        StructInfo {
            handle: handle_idx,
            def: def_idx,
            fields: field_handles,
        }
    }

    pub fn add_struct(
        &mut self,
        module_index: ModuleHandleIndex,
        name: &str,
        abilities: AbilitySet,
        fields: Vec<(&str, SignatureToken)>,
    ) -> StructInfo {
        self.add_struct_verbose(module_index, name, abilities, fields, vec![])
    }

    pub fn add_module(&mut self, address: AccountAddress, name: &str) -> ModuleHandleIndex {
        let handle = ModuleHandle {
            address: self.add_address(address),
            name: self.add_identifier(name),
        };
        self.module.module_handles.push(handle);
        ModuleHandleIndex((self.module.module_handles.len() - 1) as u16)
    }

    fn create_field(&mut self, name: &str, ty: SignatureToken) -> FieldDefinition {
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

    pub fn add_field_instantiation(
        &mut self,
        handle: FieldHandleIndex,
        type_params: Vec<SignatureToken>,
    ) -> FieldInstantiationIndex {
        let type_parameters = self.add_signature(type_params);
        self.module.field_instantiations.push(FieldInstantiation {
            handle,
            type_parameters,
        });
        FieldInstantiationIndex((self.module.field_instantiations.len() - 1) as u16)
    }

    fn add_field_handle(
        &mut self,
        struct_def: StructDefinitionIndex,
        field: u16,
    ) -> FieldHandleIndex {
        self.module.field_handles.push(FieldHandle {
            owner: struct_def,
            field,
        });
        FieldHandleIndex((self.module.field_handles.len() - 1) as u16)
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
