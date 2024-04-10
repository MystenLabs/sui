// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    access::ModuleAccess,
    control_flow_graph::VMControlFlowGraph,
    errors::PartialVMResult,
    file_format::{
        AbilitySet, AddressIdentifierIndex, CodeUnit, Constant, ConstantPoolIndex, FieldHandle,
        FieldHandleIndex, FieldInstantiation, FieldInstantiationIndex, FunctionDefinition,
        FunctionDefinitionIndex, FunctionHandle, FunctionHandleIndex, FunctionInstantiation,
        FunctionInstantiationIndex, IdentifierIndex, ModuleHandle, ModuleHandleIndex, Signature,
        SignatureIndex, SignatureToken, StructDefInstantiation, StructDefInstantiationIndex,
        StructDefinition, StructDefinitionIndex, StructHandle, StructHandleIndex,
    },
    CompiledModule,
};
use move_core_types::{
    account_address::AccountAddress,
    identifier::{IdentStr, Identifier},
    language_storage::ModuleId,
};

// A `BinaryIndexedView` provides table indexed access for both `CompiledModule` and
// `CompiledScript`.
// Operations that are not allowed for `CompiledScript` return an error.
// A typical use of a `BinaryIndexedView` is while resolving indexes in bytecodes.
#[derive(Clone, Copy, Debug)]
pub enum BinaryIndexedView<'a> {
    Module(&'a CompiledModule),
}

impl<'a> BinaryIndexedView<'a> {
    pub fn module_handles(&self) -> &[ModuleHandle] {
        match self {
            BinaryIndexedView::Module(module) => module.module_handles(),
        }
    }

    pub fn struct_handles(&self) -> &[StructHandle] {
        match self {
            BinaryIndexedView::Module(module) => module.struct_handles(),
        }
    }

    pub fn function_handles(&self) -> &[FunctionHandle] {
        match self {
            BinaryIndexedView::Module(module) => module.function_handles(),
        }
    }

    pub fn identifiers(&self) -> &[Identifier] {
        match self {
            BinaryIndexedView::Module(module) => module.identifiers(),
        }
    }

    pub fn identifier_at(&self, idx: IdentifierIndex) -> &IdentStr {
        match self {
            BinaryIndexedView::Module(module) => module.identifier_at(idx),
        }
    }

    pub fn address_identifiers(&self) -> &[AccountAddress] {
        match self {
            BinaryIndexedView::Module(module) => module.address_identifiers(),
        }
    }

    pub fn address_identifier_at(&self, idx: AddressIdentifierIndex) -> &AccountAddress {
        match self {
            BinaryIndexedView::Module(module) => module.address_identifier_at(idx),
        }
    }

    pub fn constant_pool(&self) -> &[Constant] {
        match self {
            BinaryIndexedView::Module(module) => module.constant_pool(),
        }
    }

    pub fn constant_at(&self, idx: ConstantPoolIndex) -> &Constant {
        match self {
            BinaryIndexedView::Module(module) => module.constant_at(idx),
        }
    }

    pub fn signatures(&self) -> &[Signature] {
        match self {
            BinaryIndexedView::Module(module) => module.signatures(),
        }
    }

    pub fn signature_at(&self, idx: SignatureIndex) -> &Signature {
        match self {
            BinaryIndexedView::Module(module) => module.signature_at(idx),
        }
    }

    pub fn module_handle_at(&self, idx: ModuleHandleIndex) -> &ModuleHandle {
        match self {
            BinaryIndexedView::Module(module) => module.module_handle_at(idx),
        }
    }

    pub fn struct_handle_at(&self, idx: StructHandleIndex) -> &StructHandle {
        match self {
            BinaryIndexedView::Module(module) => module.struct_handle_at(idx),
        }
    }

    pub fn function_handle_at(&self, idx: FunctionHandleIndex) -> &FunctionHandle {
        match self {
            BinaryIndexedView::Module(module) => module.function_handle_at(idx),
        }
    }

    pub fn function_instantiations(&self) -> &[FunctionInstantiation] {
        match self {
            BinaryIndexedView::Module(module) => module.function_instantiations(),
        }
    }

    pub fn function_instantiation_at(
        &self,
        idx: FunctionInstantiationIndex,
    ) -> &FunctionInstantiation {
        match self {
            BinaryIndexedView::Module(module) => module.function_instantiation_at(idx),
        }
    }

    pub fn field_handles(&self) -> Option<&[FieldHandle]> {
        match self {
            BinaryIndexedView::Module(module) => Some(module.field_handles()),
        }
    }

    pub fn field_handle_at(&self, idx: FieldHandleIndex) -> PartialVMResult<&FieldHandle> {
        match self {
            BinaryIndexedView::Module(module) => Ok(module.field_handle_at(idx)),
        }
    }

    pub fn friend_decls(&self) -> Option<&[ModuleHandle]> {
        match self {
            BinaryIndexedView::Module(module) => Some(module.friend_decls()),
        }
    }

    pub fn struct_instantiations(&self) -> Option<&[StructDefInstantiation]> {
        match self {
            BinaryIndexedView::Module(module) => Some(module.struct_instantiations()),
        }
    }

    pub fn struct_instantiation_at(
        &self,
        idx: StructDefInstantiationIndex,
    ) -> PartialVMResult<&StructDefInstantiation> {
        match self {
            BinaryIndexedView::Module(module) => Ok(module.struct_instantiation_at(idx)),
        }
    }

    pub fn field_instantiations(&self) -> Option<&[FieldInstantiation]> {
        match self {
            BinaryIndexedView::Module(module) => Some(module.field_instantiations()),
        }
    }

    pub fn field_instantiation_at(
        &self,
        idx: FieldInstantiationIndex,
    ) -> PartialVMResult<&FieldInstantiation> {
        match self {
            BinaryIndexedView::Module(module) => Ok(module.field_instantiation_at(idx)),
        }
    }

    pub fn struct_defs(&self) -> Option<&[StructDefinition]> {
        match self {
            BinaryIndexedView::Module(module) => Some(module.struct_defs()),
        }
    }

    pub fn struct_def_at(&self, idx: StructDefinitionIndex) -> PartialVMResult<&StructDefinition> {
        match self {
            BinaryIndexedView::Module(module) => Ok(module.struct_def_at(idx)),
        }
    }

    pub fn function_defs(&self) -> Option<&[FunctionDefinition]> {
        match self {
            BinaryIndexedView::Module(module) => Some(module.function_defs()),
        }
    }

    pub fn function_def_at(
        &self,
        idx: FunctionDefinitionIndex,
    ) -> PartialVMResult<&FunctionDefinition> {
        match self {
            BinaryIndexedView::Module(module) => Ok(module.function_def_at(idx)),
        }
    }

    // Return the `AbilitySet` of a `SignatureToken` given a context.
    // A `TypeParameter` has the abilities of its `constraints`.
    // `StructInstantiation` abilities are predicated on the particular instantiation
    pub fn abilities(
        &self,
        ty: &SignatureToken,
        constraints: &[AbilitySet],
    ) -> PartialVMResult<AbilitySet> {
        use SignatureToken::*;

        match ty {
            Bool | U8 | U16 | U32 | U64 | U128 | U256 | Address => Ok(AbilitySet::PRIMITIVES),

            Reference(_) | MutableReference(_) => Ok(AbilitySet::REFERENCES),
            Signer => Ok(AbilitySet::SIGNER),
            TypeParameter(idx) => Ok(constraints[*idx as usize]),
            Vector(ty) => AbilitySet::polymorphic_abilities(
                AbilitySet::VECTOR,
                vec![false],
                vec![self.abilities(ty, constraints)?],
            ),
            Struct(idx) => {
                let sh = self.struct_handle_at(*idx);
                Ok(sh.abilities)
            }
            StructInstantiation(struct_inst) => {
                let (idx, type_args) = &**struct_inst;
                let sh = self.struct_handle_at(*idx);
                let declared_abilities = sh.abilities;
                let type_arguments = type_args
                    .iter()
                    .map(|arg| self.abilities(arg, constraints))
                    .collect::<PartialVMResult<Vec<_>>>()?;
                AbilitySet::polymorphic_abilities(
                    declared_abilities,
                    sh.type_parameters.iter().map(|param| param.is_phantom),
                    type_arguments,
                )
            }
        }
    }

    pub fn self_handle_idx(&self) -> Option<ModuleHandleIndex> {
        match self {
            BinaryIndexedView::Module(m) => Some(m.self_handle_idx()),
        }
    }

    pub fn module_id_for_handle(&self, module_handle: &ModuleHandle) -> ModuleId {
        ModuleId::new(
            *self.address_identifier_at(module_handle.address),
            self.identifier_at(module_handle.name).to_owned(),
        )
    }

    pub fn self_id(&self) -> Option<ModuleId> {
        match self {
            BinaryIndexedView::Module(m) => Some(m.self_id()),
        }
    }

    pub fn version(&self) -> u32 {
        match self {
            BinaryIndexedView::Module(module) => module.version(),
        }
    }
}

// A `FunctionView` holds all the information needed by the verifier for a
// `FunctionDefinition` and its `FunctionHandle` in a single view.
// A control flow graph is built for a function when the `FunctionView` is
// created.
// A `FunctionView` is created for all module functions except native functions.
// It is also created for a script.
pub struct FunctionView<'a> {
    index: Option<FunctionDefinitionIndex>,
    code: &'a CodeUnit,
    parameters: &'a Signature,
    return_: &'a Signature,
    locals: &'a Signature,
    type_parameters: &'a [AbilitySet],
    cfg: VMControlFlowGraph,
}

impl<'a> FunctionView<'a> {
    // Creates a `FunctionView` for a module function.
    pub fn function(
        module: &'a CompiledModule,
        index: FunctionDefinitionIndex,
        code: &'a CodeUnit,
        function_handle: &'a FunctionHandle,
    ) -> Self {
        Self {
            index: Some(index),
            code,
            parameters: module.signature_at(function_handle.parameters),
            return_: module.signature_at(function_handle.return_),
            locals: module.signature_at(code.locals),
            type_parameters: &function_handle.type_parameters,
            cfg: VMControlFlowGraph::new(&code.code),
        }
    }

    pub fn index(&self) -> Option<FunctionDefinitionIndex> {
        self.index
    }

    pub fn code(&self) -> &CodeUnit {
        self.code
    }

    pub fn parameters(&self) -> &Signature {
        self.parameters
    }

    pub fn return_(&self) -> &Signature {
        self.return_
    }

    pub fn locals(&self) -> &Signature {
        self.locals
    }

    pub fn type_parameters(&self) -> &[AbilitySet] {
        self.type_parameters
    }

    pub fn cfg(&self) -> &VMControlFlowGraph {
        &self.cfg
    }
}
