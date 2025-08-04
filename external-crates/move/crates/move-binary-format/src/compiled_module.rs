// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

/// A `CompiledModule` defines the structure of a module which is the unit of published code.
///
/// A `CompiledModule` contains a definition of types (with their fields) and functions.
/// It is a unit of code that can be used by transactions or other modules.
///
/// A module is published as a single entry and it is retrieved as a single blob.
use crate::{
    errors::PartialVMResult, file_format::*, file_format_common, internals::ModuleIndex, IndexKind,
};

#[cfg(any(test, feature = "fuzzing"))]
use proptest::{collection::vec, prelude::*, strategy::BoxedStrategy};

use move_core_types::{
    account_address::AccountAddress,
    identifier::{IdentStr, Identifier},
    language_storage::ModuleId,
    metadata::Metadata,
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "fuzzing", derive(arbitrary::Arbitrary))]
#[cfg_attr(feature = "wasm", derive(Serialize, Deserialize))]
pub struct CompiledModule {
    /// Version number found during deserialization
    pub version: u32,
    /// Handle to self.
    pub self_module_handle_idx: ModuleHandleIndex,
    /// Handles to external dependency modules and self.
    pub module_handles: Vec<ModuleHandle>,
    /// Handles to external and internal types.
    pub datatype_handles: Vec<DatatypeHandle>,
    /// Handles to external and internal functions.
    pub function_handles: Vec<FunctionHandle>,
    /// Handles to fields.
    pub field_handles: Vec<FieldHandle>,
    /// Friend declarations, represented as a collection of handles to external friend modules.
    pub friend_decls: Vec<ModuleHandle>,

    /// Struct instantiations.
    pub struct_def_instantiations: Vec<StructDefInstantiation>,
    /// Function instantiations.
    pub function_instantiations: Vec<FunctionInstantiation>,
    /// Field instantiations.
    pub field_instantiations: Vec<FieldInstantiation>,

    /// Locals signature pool. The signature for all locals of the functions defined in the module.
    pub signatures: SignaturePool,

    /// All identifiers used in this module.
    pub identifiers: IdentifierPool,
    /// All address identifiers used in this module.
    pub address_identifiers: AddressIdentifierPool,
    /// Constant pool. The constant values used in the module.
    pub constant_pool: ConstantPool,

    pub metadata: Vec<Metadata>,

    /// Struc types defined in this module.
    pub struct_defs: Vec<StructDefinition>,
    /// Function defined in this module.
    pub function_defs: Vec<FunctionDefinition>,

    /// Enum types defined in this module.
    pub enum_defs: Vec<EnumDefinition>,
    /// Enum instantiations.
    pub enum_def_instantiations: Vec<EnumDefInstantiation>,
    // Enum packs
    pub variant_handles: Vec<VariantHandle>,
    // Enum pack instantiations
    pub variant_instantiation_handles: Vec<VariantInstantiationHandle>,
}

#[cfg(any(test, feature = "fuzzing"))]
impl Arbitrary for CompiledModule {
    type Strategy = BoxedStrategy<Self>;
    /// The size of the compiled module.
    type Parameters = usize;

    fn arbitrary_with(size: Self::Parameters) -> Self::Strategy {
        (
            (
                vec(any::<ModuleHandle>(), 0..=size),
                vec(any::<DatatypeHandle>(), 0..=size),
                vec(any::<FunctionHandle>(), 0..=size),
            ),
            any::<ModuleHandleIndex>(),
            vec(any::<ModuleHandle>(), 0..=size),
            vec(any_with::<Signature>(size), 0..=size),
            (
                vec(any::<Identifier>(), 0..=size),
                vec(any::<AccountAddress>(), 0..=size),
            ),
            (
                vec(any::<StructDefinition>(), 0..=size),
                vec(any_with::<FunctionDefinition>(size), 0..=size),
            ),
        )
            .prop_map(
                |(
                    (module_handles, datatype_handles, function_handles),
                    self_module_handle_idx,
                    friend_decls,
                    signatures,
                    (identifiers, address_identifiers),
                    (struct_defs, function_defs),
                )| {
                    // TODO actual constant generation
                    CompiledModule {
                        version: file_format_common::VERSION_MAX,
                        module_handles,
                        datatype_handles,
                        function_handles,
                        self_module_handle_idx,
                        field_handles: vec![],
                        friend_decls,
                        struct_def_instantiations: vec![],
                        function_instantiations: vec![],
                        field_instantiations: vec![],
                        signatures,
                        identifiers,
                        address_identifiers,
                        constant_pool: vec![],
                        metadata: vec![],
                        struct_defs,
                        function_defs,
                        enum_defs: vec![],
                        enum_def_instantiations: vec![],
                        variant_handles: vec![],
                        variant_instantiation_handles: vec![],
                    }
                },
            )
            .boxed()
    }
}

impl CompiledModule {
    /// Returns the count of a specific `IndexKind`
    pub fn kind_count(&self, kind: IndexKind) -> usize {
        debug_assert!(!matches!(
            kind,
            IndexKind::LocalPool
                | IndexKind::CodeDefinition
                | IndexKind::FieldDefinition
                | IndexKind::TypeParameter
                | IndexKind::MemberCount
                | IndexKind::VariantTag
                | IndexKind::VariantJumpTable
        ));
        match kind {
            IndexKind::ModuleHandle => self.module_handles.len(),
            IndexKind::DatatypeHandle => self.datatype_handles.len(),
            IndexKind::FunctionHandle => self.function_handles.len(),
            IndexKind::FieldHandle => self.field_handles.len(),
            IndexKind::FriendDeclaration => self.friend_decls.len(),
            IndexKind::StructDefInstantiation => self.struct_def_instantiations.len(),
            IndexKind::FunctionInstantiation => self.function_instantiations.len(),
            IndexKind::FieldInstantiation => self.field_instantiations.len(),
            IndexKind::StructDefinition => self.struct_defs.len(),
            IndexKind::FunctionDefinition => self.function_defs.len(),
            IndexKind::Signature => self.signatures.len(),
            IndexKind::Identifier => self.identifiers.len(),
            IndexKind::AddressIdentifier => self.address_identifiers.len(),
            IndexKind::ConstantPool => self.constant_pool.len(),
            IndexKind::EnumDefinition => self.enum_defs.len(),
            IndexKind::EnumDefInstantiation => self.enum_def_instantiations.len(),
            IndexKind::VariantHandle => self.variant_handles.len(),
            IndexKind::VariantInstantiationHandle => self.variant_instantiation_handles.len(),
            // XXX these two don't seem to belong here
            other @ (IndexKind::LocalPool
            | IndexKind::CodeDefinition
            | IndexKind::FieldDefinition
            | IndexKind::TypeParameter
            | IndexKind::VariantTag
            | IndexKind::VariantJumpTable
            | IndexKind::MemberCount) => unreachable!("invalid kind for count: {:?}", other),
        }
    }

    pub fn self_handle_idx(&self) -> ModuleHandleIndex {
        self.self_module_handle_idx
    }

    /// Returns the `ModuleHandle` for `self`.
    pub fn self_handle(&self) -> &ModuleHandle {
        let handle = self.module_handle_at(self.self_handle_idx());
        debug_assert!(handle.address.into_index() < self.address_identifiers.len()); // invariant
        debug_assert!(handle.name.into_index() < self.identifiers.len()); // invariant
        handle
    }

    /// Returns the name of the module.
    pub fn name(&self) -> &IdentStr {
        self.identifier_at(self.self_handle().name)
    }

    /// Returns the address of the module.
    pub fn address(&self) -> &AccountAddress {
        self.address_identifier_at(self.self_handle().address)
    }

    pub fn struct_name(&self, idx: StructDefinitionIndex) -> &IdentStr {
        let struct_def = self.struct_def_at(idx);
        let handle = self.datatype_handle_at(struct_def.struct_handle);
        self.identifier_at(handle.name)
    }

    pub fn enum_name(&self, idx: EnumDefinitionIndex) -> &IdentStr {
        let enum_def = self.enum_def_at(idx);
        let handle = self.datatype_handle_at(enum_def.enum_handle);
        self.identifier_at(handle.name)
    }

    pub fn module_handle_at(&self, idx: ModuleHandleIndex) -> &ModuleHandle {
        let handle = &self.module_handles[idx.into_index()];
        debug_assert!(handle.address.into_index() < self.address_identifiers.len()); // invariant
        debug_assert!(handle.name.into_index() < self.identifiers.len()); // invariant
        handle
    }

    pub fn datatype_handle_at(&self, idx: DatatypeHandleIndex) -> &DatatypeHandle {
        let handle = &self.datatype_handles[idx.into_index()];
        debug_assert!(handle.module.into_index() < self.module_handles.len()); // invariant
        handle
    }

    pub fn function_handle_at(&self, idx: FunctionHandleIndex) -> &FunctionHandle {
        let handle = &self.function_handles[idx.into_index()];
        debug_assert!(handle.parameters.into_index() < self.signatures.len()); // invariant
        debug_assert!(handle.return_.into_index() < self.signatures.len()); // invariant
        handle
    }

    pub fn field_handle_at(&self, idx: FieldHandleIndex) -> &FieldHandle {
        let handle = &self.field_handles[idx.into_index()];
        debug_assert!(handle.owner.into_index() < self.struct_defs.len()); // invariant
        handle
    }

    pub fn variant_handle_at(&self, idx: VariantHandleIndex) -> &VariantHandle {
        let handle = &self.variant_handles[idx.into_index()];
        debug_assert!(handle.enum_def.into_index() < self.enum_defs.len());
        handle
    }

    pub fn variant_instantiation_handle_at(
        &self,
        idx: VariantInstantiationHandleIndex,
    ) -> &VariantInstantiationHandle {
        let handle = &self.variant_instantiation_handles[idx.into_index()];
        debug_assert!(handle.enum_def.into_index() < self.enum_def_instantiations.len()); // invariant
        handle
    }

    pub fn struct_instantiation_at(
        &self,
        idx: StructDefInstantiationIndex,
    ) -> &StructDefInstantiation {
        &self.struct_def_instantiations[idx.into_index()]
    }

    pub fn enum_instantiation_at(&self, idx: EnumDefInstantiationIndex) -> &EnumDefInstantiation {
        &self.enum_def_instantiations[idx.into_index()]
    }

    pub fn function_instantiation_at(
        &self,
        idx: FunctionInstantiationIndex,
    ) -> &FunctionInstantiation {
        &self.function_instantiations[idx.into_index()]
    }

    pub fn field_instantiation_at(&self, idx: FieldInstantiationIndex) -> &FieldInstantiation {
        &self.field_instantiations[idx.into_index()]
    }

    pub fn signature_at(&self, idx: SignatureIndex) -> &Signature {
        &self.signatures[idx.into_index()]
    }

    pub fn identifier_at(&self, idx: IdentifierIndex) -> &IdentStr {
        &self.identifiers[idx.into_index()]
    }

    pub fn address_identifier_at(&self, idx: AddressIdentifierIndex) -> &AccountAddress {
        &self.address_identifiers[idx.into_index()]
    }

    pub fn constant_at(&self, idx: ConstantPoolIndex) -> &Constant {
        &self.constant_pool[idx.into_index()]
    }

    pub fn struct_def_at(&self, idx: StructDefinitionIndex) -> &StructDefinition {
        &self.struct_defs[idx.into_index()]
    }

    pub fn enum_def_at(&self, idx: EnumDefinitionIndex) -> &EnumDefinition {
        &self.enum_defs[idx.into_index()]
    }

    pub fn variant_def_at(
        &self,
        enum_idx: EnumDefinitionIndex,
        variant_tag: VariantTag,
    ) -> &VariantDefinition {
        // invariant
        debug_assert!(self.enum_def_at(enum_idx).variants.len() > variant_tag as usize);
        &self.enum_def_at(enum_idx).variants[variant_tag as usize]
    }

    pub fn function_def_at(&self, idx: FunctionDefinitionIndex) -> &FunctionDefinition {
        let result = &self.function_defs[idx.into_index()];
        debug_assert!(result.function.into_index() < self.function_handles().len()); // invariant
        debug_assert!(match &result.code {
            Some(code) => code.locals.into_index() < self.signatures().len(),
            None => true,
        }); // invariant
        result
    }

    pub fn find_function_def_by_name(
        &self,
        name: impl AsRef<str>,
    ) -> Option<(FunctionDefinitionIndex, &FunctionDefinition)> {
        let name: &str = name.as_ref();
        self.function_defs()
            .iter()
            .enumerate()
            .find_map(|(idx, def)| {
                let handle = self.function_handle_at(def.function);
                if name == self.identifier_at(handle.name).as_str() {
                    Some((FunctionDefinitionIndex::new(idx as TableIndex), def))
                } else {
                    None
                }
            })
    }

    pub fn module_handles(&self) -> &[ModuleHandle] {
        &self.module_handles
    }

    pub fn datatype_handles(&self) -> &[DatatypeHandle] {
        &self.datatype_handles
    }

    pub fn function_handles(&self) -> &[FunctionHandle] {
        &self.function_handles
    }

    pub fn field_handles(&self) -> &[FieldHandle] {
        &self.field_handles
    }

    pub fn struct_instantiations(&self) -> &[StructDefInstantiation] {
        &self.struct_def_instantiations
    }

    pub fn enum_instantiations(&self) -> &[EnumDefInstantiation] {
        &self.enum_def_instantiations
    }

    pub fn function_instantiations(&self) -> &[FunctionInstantiation] {
        &self.function_instantiations
    }

    pub fn field_instantiations(&self) -> &[FieldInstantiation] {
        &self.field_instantiations
    }

    pub fn signatures(&self) -> &[Signature] {
        &self.signatures
    }

    pub fn constant_pool(&self) -> &[Constant] {
        &self.constant_pool
    }

    pub fn identifiers(&self) -> &[Identifier] {
        &self.identifiers
    }

    pub fn address_identifiers(&self) -> &[AccountAddress] {
        &self.address_identifiers
    }

    pub fn struct_defs(&self) -> &[StructDefinition] {
        &self.struct_defs
    }

    pub fn enum_defs(&self) -> &[EnumDefinition] {
        &self.enum_defs
    }

    pub fn variant_handles(&self) -> &[VariantHandle] {
        &self.variant_handles
    }

    pub fn variant_instantiation_handles(&self) -> &[VariantInstantiationHandle] {
        &self.variant_instantiation_handles
    }

    pub fn function_defs(&self) -> &[FunctionDefinition] {
        &self.function_defs
    }

    pub fn friend_decls(&self) -> &[ModuleHandle] {
        &self.friend_decls
    }

    pub fn version(&self) -> u32 {
        self.version
    }

    pub fn immediate_dependencies(&self) -> Vec<ModuleId> {
        let self_handle = self.self_handle();
        self.module_handles()
            .iter()
            .filter(|&handle| handle != self_handle)
            .map(|handle| self.module_id_for_handle(handle))
            .collect()
    }

    pub fn immediate_friends(&self) -> Vec<ModuleId> {
        self.friend_decls()
            .iter()
            .map(|handle| self.module_id_for_handle(handle))
            .collect()
    }

    pub fn find_struct_def(&self, idx: DatatypeHandleIndex) -> Option<&StructDefinition> {
        self.struct_defs().iter().find(|d| d.struct_handle == idx)
    }

    pub fn find_enum_def(&self, idx: DatatypeHandleIndex) -> Option<&EnumDefinition> {
        self.enum_defs().iter().find(|d| d.enum_handle == idx)
    }

    pub fn find_struct_def_by_name(
        &self,
        name: &str,
    ) -> Option<(StructDefinitionIndex, &StructDefinition)> {
        self.struct_defs()
            .iter()
            .enumerate()
            .find(|(_idx, def)| {
                let handle = self.datatype_handle_at(def.struct_handle);
                name == self.identifier_at(handle.name).as_str()
            })
            .map(|(idx, def)| (StructDefinitionIndex(idx as TableIndex), def))
    }

    pub fn find_enum_def_by_name(
        &self,
        name: &str,
    ) -> Option<(EnumDefinitionIndex, &EnumDefinition)> {
        self.enum_defs()
            .iter()
            .enumerate()
            .find(|(_idx, def)| {
                let handle = self.datatype_handle_at(def.enum_handle);
                name == self.identifier_at(handle.name).as_str()
            })
            .map(|(idx, def)| (EnumDefinitionIndex(idx as TableIndex), def))
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
            Datatype(idx) => {
                let sh = self.datatype_handle_at(*idx);
                Ok(sh.abilities)
            }
            DatatypeInstantiation(inst) => {
                let (idx, type_args) = &**inst;
                let sh = self.datatype_handle_at(*idx);
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

    /// Returns the code key of `module_handle`
    pub fn module_id_for_handle(&self, module_handle: &ModuleHandle) -> ModuleId {
        ModuleId::new(
            *self.address_identifier_at(module_handle.address),
            self.identifier_at(module_handle.name).to_owned(),
        )
    }

    /// Returns the code key of `self`
    pub fn self_id(&self) -> ModuleId {
        self.module_id_for_handle(self.self_handle())
    }
}

/// Return the simplest module that will pass the bounds checker
pub fn empty_module() -> CompiledModule {
    CompiledModule {
        version: file_format_common::VERSION_MAX,
        module_handles: vec![ModuleHandle {
            address: AddressIdentifierIndex(0),
            name: IdentifierIndex(0),
        }],
        self_module_handle_idx: ModuleHandleIndex(0),
        identifiers: vec![self_module_name().to_owned()],
        address_identifiers: vec![AccountAddress::ZERO],
        constant_pool: vec![],
        metadata: vec![],
        function_defs: vec![],
        struct_defs: vec![],
        datatype_handles: vec![],
        function_handles: vec![],
        field_handles: vec![],
        friend_decls: vec![],
        struct_def_instantiations: vec![],
        function_instantiations: vec![],
        field_instantiations: vec![],
        signatures: vec![Signature(vec![])],
        enum_defs: vec![],
        enum_def_instantiations: vec![],
        variant_handles: vec![],
        variant_instantiation_handles: vec![],
    }
}

/// Create the following module which is convenient in tests:
/// // module <SELF> {
/// //     struct Bar { x: u64 }
/// //
/// //     foo() {
/// //     }
/// // }
pub fn basic_test_module() -> CompiledModule {
    let mut m = empty_module();

    m.function_handles.push(FunctionHandle {
        module: ModuleHandleIndex(0),
        name: IdentifierIndex(m.identifiers.len() as u16),
        parameters: SignatureIndex(0),
        return_: SignatureIndex(0),
        type_parameters: vec![],
    });
    m.identifiers
        .push(Identifier::new("foo".to_string()).unwrap());

    m.function_defs.push(FunctionDefinition {
        function: FunctionHandleIndex(0),
        visibility: Visibility::Private,
        is_entry: false,
        acquires_global_resources: vec![],
        code: Some(CodeUnit {
            locals: SignatureIndex(0),
            code: vec![Bytecode::Ret],
            jump_tables: vec![],
        }),
    });

    m.datatype_handles.push(DatatypeHandle {
        module: ModuleHandleIndex(0),
        name: IdentifierIndex(m.identifiers.len() as u16),
        abilities: AbilitySet::EMPTY,
        type_parameters: vec![],
    });
    m.identifiers
        .push(Identifier::new("Bar".to_string()).unwrap());

    m.struct_defs.push(StructDefinition {
        struct_handle: DatatypeHandleIndex(0),
        field_information: StructFieldInformation::Declared(vec![FieldDefinition {
            name: IdentifierIndex(m.identifiers.len() as u16),
            signature: TypeSignature(SignatureToken::U64),
        }]),
    });
    m.identifiers
        .push(Identifier::new("x".to_string()).unwrap());
    m
}

pub fn basic_test_module_with_enum() -> CompiledModule {
    let mut m = basic_test_module();
    m.datatype_handles.push(DatatypeHandle {
        module: ModuleHandleIndex(0),
        name: IdentifierIndex(m.identifiers.len() as u16),
        abilities: AbilitySet::EMPTY,
        type_parameters: vec![],
    });
    m.identifiers
        .push(Identifier::new("enum".to_string()).unwrap());
    m.enum_defs.push(EnumDefinition {
        enum_handle: DatatypeHandleIndex::new(1),
        variants: vec![VariantDefinition {
            variant_name: IdentifierIndex::new(0),
            fields: vec![],
        }],
    });
    m.variant_handles.push(VariantHandle {
        enum_def: EnumDefinitionIndex::new(0),
        variant: 0,
    });
    m
}
