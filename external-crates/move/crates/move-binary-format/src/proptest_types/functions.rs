// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    file_format::{
        AbilitySet, Bytecode, CodeOffset, CodeUnit, ConstantPoolIndex, DatatypeHandle,
        EnumDefInstantiation, EnumDefInstantiationIndex, EnumDefinition, EnumDefinitionIndex,
        FieldHandle, FieldHandleIndex, FieldInstantiation, FieldInstantiationIndex,
        FunctionDefinition, FunctionHandle, FunctionHandleIndex, FunctionInstantiation,
        FunctionInstantiationIndex, IdentifierIndex, LocalIndex, ModuleHandleIndex, Signature,
        SignatureIndex, SignatureToken, StructDefInstantiation, StructDefInstantiationIndex,
        StructDefinition, StructDefinitionIndex, TableIndex, VariantJumpTable,
        VariantJumpTableIndex, Visibility,
    },
    file_format_common::VARIANT_COUNT_MAX,
    internals::ModuleIndex,
    proptest_types::{
        prop_index_avoid,
        signature::{AbilitySetGen, SignatureGen, SignatureTokenGen},
        TableSize,
    },
};
use move_core_types::u256::U256;
use proptest::{
    collection::{vec, SizeRange},
    prelude::*,
    sample::{select, Index as PropIndex},
};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    hash::Hash,
};

#[derive(Debug, Default)]
struct SignatureState {
    signatures: Vec<Signature>,
    signature_map: HashMap<Signature, SignatureIndex>,
}

impl SignatureState {
    fn new(signatures: Vec<Signature>) -> Self {
        let mut state = Self::default();
        for sig in signatures {
            state.add_signature(sig);
        }
        state
    }

    fn signatures(self) -> Vec<Signature> {
        self.signatures
    }

    fn add_signature(&mut self, sig: Signature) -> SignatureIndex {
        debug_assert!(self.signatures.len() < TableSize::max_value() as usize);
        if let Some(idx) = self.signature_map.get(&sig) {
            return *idx;
        }
        let idx = SignatureIndex(self.signatures.len() as u16);
        self.signatures.push(sig.clone());
        self.signature_map.insert(sig, idx);
        idx
    }
}

#[derive(Debug, Default)]
#[allow(unused)]
struct FieldHandleState {
    field_handles: Vec<FieldHandle>,
    field_map: HashMap<FieldHandle, FieldHandleIndex>,
}

impl FieldHandleState {
    #[allow(unused)]
    pub fn field_handles(self) -> Vec<FieldHandle> {
        self.field_handles
    }

    #[allow(unused)]
    fn add_field_handle(&mut self, fh: FieldHandle) -> FieldHandleIndex {
        debug_assert!(self.field_handles.len() < TableSize::max_value() as usize);
        if let Some(idx) = self.field_map.get(&fh) {
            return *idx;
        }
        let idx = FieldHandleIndex(self.field_handles.len() as u16);
        self.field_handles.push(fh.clone());
        self.field_map.insert(fh, idx);
        idx
    }
}

#[derive(Debug)]
#[allow(unused)]
struct InstantiationState<T>
where
    T: Eq + Clone + Hash,
{
    instantiations: Vec<T>,
    instantiation_map: HashMap<T, TableIndex>,
}

impl<T> InstantiationState<T>
where
    T: Eq + Clone + Hash,
{
    fn new() -> Self {
        InstantiationState {
            instantiations: vec![],
            instantiation_map: HashMap::new(),
        }
    }

    #[allow(unused)]
    pub fn instantiations(self) -> Vec<T> {
        self.instantiations
    }

    #[allow(unused)]
    fn add_instantiation(&mut self, inst: T) -> TableIndex {
        debug_assert!(self.instantiations.len() < TableSize::max_value() as usize);
        if let Some(idx) = self.instantiation_map.get(&inst) {
            return *idx;
        }
        let idx = self.instantiations.len() as TableIndex;
        self.instantiations.push(inst.clone());
        self.instantiation_map.insert(inst, idx);
        idx
    }
}

/// Represents state required to materialize final data structures for function handles.
#[derive(Debug)]
pub struct FnHandleMaterializeState<'a> {
    self_module_handle_idx: ModuleHandleIndex,
    module_handles_len: usize,
    identifiers_len: usize,
    datatype_handles: &'a [DatatypeHandle],
    signatures: SignatureState,
    function_handles: HashSet<(ModuleHandleIndex, IdentifierIndex)>,
}

impl<'a> FnHandleMaterializeState<'a> {
    pub fn new(
        self_module_handle_idx: ModuleHandleIndex,
        module_handles_len: usize,
        identifiers_len: usize,
        datatype_handles: &'a [DatatypeHandle],
        signatures: Vec<Signature>,
    ) -> Self {
        Self {
            self_module_handle_idx,
            module_handles_len,
            identifiers_len,
            datatype_handles,
            signatures: SignatureState::new(signatures),
            function_handles: HashSet::new(),
        }
    }

    pub fn signatures(self) -> Vec<Signature> {
        self.signatures.signatures()
    }

    fn add_signature(&mut self, sig: Signature) -> SignatureIndex {
        self.signatures.add_signature(sig)
    }
}

#[derive(Clone, Debug)]
pub struct FunctionHandleGen {
    module: PropIndex,
    name: PropIndex,
    parameters: SignatureGen,
    return_: SignatureGen,
    type_parameters: Vec<AbilitySetGen>,
}

impl FunctionHandleGen {
    pub fn strategy(
        param_count: impl Into<SizeRange>,
        return_count: impl Into<SizeRange>,
        type_parameter_count: impl Into<SizeRange>,
    ) -> impl Strategy<Value = Self> {
        let return_count = return_count.into();
        let param_count = param_count.into();
        (
            any::<PropIndex>(),
            any::<PropIndex>(),
            SignatureGen::strategy(param_count),
            SignatureGen::strategy(return_count),
            vec(AbilitySetGen::strategy(), type_parameter_count),
        )
            .prop_map(
                |(module, name, parameters, return_, type_parameters)| Self {
                    module,
                    name,
                    parameters,
                    return_,
                    type_parameters,
                },
            )
    }

    pub fn materialize(self, state: &mut FnHandleMaterializeState) -> Option<FunctionHandle> {
        let idx = prop_index_avoid(
            self.module,
            state.self_module_handle_idx.into_index(),
            state.module_handles_len,
        );
        let mod_idx = ModuleHandleIndex(idx as TableIndex);
        let iden_idx = IdentifierIndex(self.name.index(state.identifiers_len) as TableIndex);
        if state.function_handles.contains(&(mod_idx, iden_idx)) {
            return None;
        }
        state.function_handles.insert((mod_idx, iden_idx));
        let parameters = self.parameters.materialize(state.datatype_handles);
        let params_idx = state.add_signature(parameters);
        let return_ = self.return_.materialize(state.datatype_handles);
        let return_idx = state.add_signature(return_);
        let type_parameters = self
            .type_parameters
            .into_iter()
            .map(|abilities| abilities.materialize())
            .collect();
        Some(FunctionHandle {
            module: mod_idx,
            name: iden_idx,
            parameters: params_idx,
            return_: return_idx,
            type_parameters,
        })
    }
}

/// Represents state required to materialize final data structures for function definitions.
#[derive(Debug)]
pub struct FnDefnMaterializeState<'a> {
    self_module_handle_idx: ModuleHandleIndex,
    identifiers_len: usize,
    constant_pool_len: usize,
    datatype_handles: &'a [DatatypeHandle],
    struct_defs: &'a [StructDefinition],
    signatures: SignatureState,
    function_handles: Vec<FunctionHandle>,
    struct_def_to_field_count: HashMap<usize, usize>,
    def_function_handles: HashSet<(ModuleHandleIndex, IdentifierIndex)>,
    field_handles: FieldHandleState,
    struct_instantiations: InstantiationState<StructDefInstantiation>,
    function_instantiations: InstantiationState<FunctionInstantiation>,
    field_instantiations: InstantiationState<FieldInstantiation>,
    enum_defs: &'a [EnumDefinition],
    enum_instantiations: InstantiationState<EnumDefInstantiation>,
}

impl<'a> FnDefnMaterializeState<'a> {
    pub fn new(
        self_module_handle_idx: ModuleHandleIndex,
        identifiers_len: usize,
        constant_pool_len: usize,
        datatype_handles: &'a [DatatypeHandle],
        struct_defs: &'a [StructDefinition],
        signatures: Vec<Signature>,
        function_handles: Vec<FunctionHandle>,
        struct_def_to_field_count: HashMap<usize, usize>,
        enum_defs: &'a [EnumDefinition],
    ) -> Self {
        Self {
            self_module_handle_idx,
            identifiers_len,
            constant_pool_len,
            datatype_handles,
            struct_defs,
            signatures: SignatureState::new(signatures),
            function_handles,
            struct_def_to_field_count,
            def_function_handles: HashSet::new(),
            field_handles: FieldHandleState::default(),
            struct_instantiations: InstantiationState::new(),
            function_instantiations: InstantiationState::new(),
            field_instantiations: InstantiationState::new(),
            enum_defs,
            enum_instantiations: InstantiationState::new(),
        }
    }

    pub fn return_tables(
        self,
    ) -> (
        Vec<Signature>,
        Vec<FunctionHandle>,
        Vec<FieldHandle>,
        Vec<StructDefInstantiation>,
        Vec<FunctionInstantiation>,
        Vec<FieldInstantiation>,
        Vec<EnumDefInstantiation>,
    ) {
        (
            self.signatures.signatures(),
            self.function_handles,
            self.field_handles.field_handles(),
            self.struct_instantiations.instantiations(),
            self.function_instantiations.instantiations(),
            self.field_instantiations.instantiations(),
            self.enum_instantiations.instantiations(),
        )
    }

    fn add_signature(&mut self, sig: Signature) -> SignatureIndex {
        self.signatures.add_signature(sig)
    }

    fn add_function_handle(&mut self, handle: FunctionHandle) -> FunctionHandleIndex {
        debug_assert!(self.function_handles.len() < TableSize::max_value() as usize);
        self.function_handles.push(handle);
        FunctionHandleIndex((self.function_handles.len() - 1) as TableIndex)
    }

    fn get_signature_from_type_params(
        &mut self,
        abilities: impl IntoIterator<Item = AbilitySet>,
    ) -> Signature {
        let mut type_params = vec![];
        for abs in abilities {
            assert!(!abs.has_key());
            match (abs.has_copy(), abs.has_drop(), abs.has_store()) {
                (false, true, false) => type_params.push(SignatureToken::Signer),
                _ => type_params.push(SignatureToken::U64),
            }
        }
        Signature(type_params)
    }

    fn add_signature_from_type_params(
        &mut self,
        abilities: impl IntoIterator<Item = AbilitySet>,
    ) -> SignatureIndex {
        let sig = self.get_signature_from_type_params(abilities);
        self.signatures.add_signature(sig)
    }

    fn get_function_instantiation(&mut self, fh_idx: usize) -> FunctionInstantiationIndex {
        let abilities = self.function_handles[fh_idx].type_parameters.clone();
        let sig_idx = self.add_signature_from_type_params(abilities.iter().copied());
        let fi = FunctionInstantiation {
            handle: FunctionHandleIndex(fh_idx as TableIndex),
            type_parameters: sig_idx,
        };
        FunctionInstantiationIndex(self.function_instantiations.add_instantiation(fi))
    }

    fn get_type_instantiation(&mut self, sd_idx: usize) -> StructDefInstantiationIndex {
        let sd = &self.struct_defs[sd_idx];
        let struct_handle = &self.datatype_handles[sd.struct_handle.0 as usize];
        let sig_idx = self.add_signature_from_type_params(struct_handle.type_param_constraints());
        let si = StructDefInstantiation {
            def: StructDefinitionIndex(sd_idx as TableIndex),
            type_parameters: sig_idx,
        };
        StructDefInstantiationIndex(self.struct_instantiations.add_instantiation(si))
    }

    fn get_enum_type_instantiation(&mut self, sd_idx: usize) -> EnumDefInstantiationIndex {
        let sd = &self.enum_defs[sd_idx];
        let enum_handle = &self.datatype_handles[sd.enum_handle.0 as usize];
        let sig_idx = self.add_signature_from_type_params(enum_handle.type_param_constraints());
        let si = EnumDefInstantiation {
            def: EnumDefinitionIndex(sd_idx as TableIndex),
            type_parameters: sig_idx,
        };
        EnumDefInstantiationIndex(self.enum_instantiations.add_instantiation(si))
    }
}

#[derive(Clone, Debug)]
pub struct FunctionDefinitionGen {
    name: PropIndex,
    parameters: SignatureGen,
    return_: SignatureGen,
    visibility: Visibility,
    is_entry: bool,
    acquires: Vec<PropIndex>,
    code: CodeUnitGen,
}

impl FunctionDefinitionGen {
    pub fn strategy(
        return_count: impl Into<SizeRange>,
        arg_count: impl Into<SizeRange>,
        _type_parameter_count: impl Into<SizeRange>,
        acquires_count: impl Into<SizeRange>,
        code_len: impl Into<SizeRange>,
        jump_table_len: impl Into<SizeRange>,
    ) -> impl Strategy<Value = Self> {
        let return_count = return_count.into();
        let arg_count = arg_count.into();
        (
            any::<PropIndex>(),
            SignatureGen::strategy(arg_count.clone()),
            SignatureGen::strategy(return_count),
            any::<Visibility>(),
            any::<bool>(),
            vec(any::<PropIndex>(), acquires_count.into()),
            CodeUnitGen::strategy(arg_count, code_len, jump_table_len),
        )
            .prop_map(
                |(name, parameters, return_, visibility, is_entry, acquires, code)| Self {
                    name,
                    parameters,
                    return_,
                    visibility,
                    is_entry,
                    acquires,
                    code,
                },
            )
    }

    pub fn materialize(self, state: &mut FnDefnMaterializeState) -> Option<FunctionDefinition> {
        // This precondition should never fail because the table size cannot be greater
        // than TableSize::max_value()
        let iden_idx = IdentifierIndex(self.name.index(state.identifiers_len) as TableIndex);
        if state
            .def_function_handles
            .contains(&(state.self_module_handle_idx, iden_idx))
        {
            return None;
        }
        state
            .def_function_handles
            .insert((state.self_module_handle_idx, iden_idx));

        let parameters = self.parameters.materialize(state.datatype_handles);
        let params_idx = state.add_signature(parameters);
        let return_ = self.return_.materialize(state.datatype_handles);
        let return_idx = state.add_signature(return_);
        let handle = FunctionHandle {
            module: state.self_module_handle_idx,
            name: iden_idx,
            parameters: params_idx,
            return_: return_idx,
            type_parameters: vec![],
        };
        let function_handle = state.add_function_handle(handle);
        let mut acquires_set = BTreeSet::new();
        for acquire in self.acquires {
            acquires_set.insert(StructDefinitionIndex(
                acquire.index(state.struct_defs.len()) as TableIndex,
            ));
        }
        let acquires_global_resources = acquires_set.into_iter().collect();
        // TODO: consider generating native functions?
        Some(FunctionDefinition {
            function: function_handle,
            visibility: self.visibility,
            is_entry: self.is_entry,
            acquires_global_resources,
            code: Some(self.code.materialize(state)),
        })
    }
}

#[derive(Clone, Debug)]
struct VariantJumpTableGen {
    index: PropIndex,
}

impl VariantJumpTableGen {
    fn strategy() -> impl Strategy<Value = Self> {
        (any::<PropIndex>()).prop_map(|index| Self { index })
    }

    fn materialize(self, state: &mut FnDefnMaterializeState) -> Option<VariantJumpTable> {
        let enum_defs_len = state.enum_defs.len();
        if enum_defs_len == 0 {
            return None;
        }
        let sd_idx = self.index.index(enum_defs_len);
        let head_enum = EnumDefinitionIndex(sd_idx as TableIndex);
        let sd = &state.enum_defs[sd_idx];
        let jump_table = (0..sd.variants.len()).map(|_| 0).collect();
        Some(VariantJumpTable {
            head_enum,
            jump_table,
        })
    }
}

#[derive(Clone, Debug)]
struct CodeUnitGen {
    locals_signature: Vec<SignatureTokenGen>,
    code: Vec<BytecodeGen>,
    jump_tables: Vec<VariantJumpTableGen>,
}

impl CodeUnitGen {
    fn strategy(
        arg_count: impl Into<SizeRange>,
        code_len: impl Into<SizeRange>,
        jump_table_len: impl Into<SizeRange>,
    ) -> impl Strategy<Value = Self> {
        (
            vec(SignatureTokenGen::strategy(), arg_count),
            vec(BytecodeGen::garbage_strategy(), code_len),
            vec(VariantJumpTableGen::strategy(), jump_table_len),
        )
            .prop_map(|(locals_signature, code, jump_tables)| Self {
                locals_signature,
                code,
                jump_tables,
            })
    }

    fn materialize(self, state: &mut FnDefnMaterializeState) -> CodeUnit {
        let locals_signature = Signature(
            self.locals_signature
                .into_iter()
                .map(|sig| sig.materialize(state.datatype_handles))
                .collect(),
        );

        let mut jump_tables = vec![];
        for jump_table in self.jump_tables {
            if let Some(jt) = jump_table.materialize(state) {
                jump_tables.push(jt);
            }
        }

        let mut code = vec![];
        for bytecode_gen in self.code {
            if let Some(bytecode) =
                bytecode_gen.materialize(state, code.len(), &locals_signature, jump_tables.len())
            {
                code.push(bytecode)
            }
        }

        // If the code is empty then we can't jump to anything so remove any jump tables.
        if code.is_empty() {
            jump_tables = vec![];
        }

        CodeUnit {
            locals: state.add_signature(locals_signature),
            code,
            jump_tables,
        }
    }
}

#[derive(Clone, Debug)]
enum BytecodeGen {
    // "Simple" means this doesn't refer to any other indexes.
    Simple(Bytecode),
    // All of these refer to other indexes.
    LdConst(PropIndex),

    MutBorrowField((PropIndex, PropIndex)),
    ImmBorrowField((PropIndex, PropIndex)),

    Call(PropIndex),

    Pack(PropIndex),
    Unpack(PropIndex),
    Exists(PropIndex),
    MutBorrowGlobal(PropIndex),
    ImmBorrowGlobal(PropIndex),
    MoveFrom(PropIndex),
    MoveTo(PropIndex),
    BrTrue(PropIndex),
    BrFalse(PropIndex),
    Branch(PropIndex),
    CopyLoc(PropIndex),
    MoveLoc(PropIndex),
    StLoc(PropIndex),
    MutBorrowLoc(PropIndex),
    ImmBorrowLoc(PropIndex),

    VecPack((PropIndex, u64)),
    VecLen(PropIndex),
    VecImmBorrow(PropIndex),
    VecMutBorrow(PropIndex),
    VecPushBack(PropIndex),
    VecPopBack(PropIndex),
    VecUnpack((PropIndex, u64)),
    VecSwap(PropIndex),

    PackVariant(PropIndex, u16),
    UnpackVariant(PropIndex, u16),
    UnpackVariantImmRef(PropIndex, u16),
    UnpackVariantMutRef(PropIndex, u16),
    VariantSwitch(PropIndex),
}

impl BytecodeGen {
    // This just generates nonsensical bytecodes. This will be cleaned up later as the generation
    // model is refined.
    fn garbage_strategy() -> impl Strategy<Value = Self> {
        use BytecodeGen::*;

        prop_oneof![
            Self::simple_bytecode_strategy().prop_map(Simple),
            any::<PropIndex>().prop_map(LdConst),
            (any::<PropIndex>(), any::<PropIndex>()).prop_map(ImmBorrowField),
            (any::<PropIndex>(), any::<PropIndex>()).prop_map(MutBorrowField),
            any::<PropIndex>().prop_map(Call),
            any::<PropIndex>().prop_map(Pack),
            any::<PropIndex>().prop_map(Unpack),
            any::<PropIndex>().prop_map(Exists),
            any::<PropIndex>().prop_map(ImmBorrowGlobal),
            any::<PropIndex>().prop_map(MutBorrowGlobal),
            any::<PropIndex>().prop_map(MoveFrom),
            any::<PropIndex>().prop_map(MoveTo),
            any::<PropIndex>().prop_map(BrTrue),
            any::<PropIndex>().prop_map(BrFalse),
            any::<PropIndex>().prop_map(Branch),
            any::<PropIndex>().prop_map(CopyLoc),
            any::<PropIndex>().prop_map(MoveLoc),
            any::<PropIndex>().prop_map(StLoc),
            any::<PropIndex>().prop_map(MutBorrowLoc),
            any::<PropIndex>().prop_map(ImmBorrowLoc),
            (any::<PropIndex>(), any::<u64>()).prop_map(VecPack),
            any::<PropIndex>().prop_map(VecLen),
            any::<PropIndex>().prop_map(VecImmBorrow),
            any::<PropIndex>().prop_map(VecMutBorrow),
            any::<PropIndex>().prop_map(VecPushBack),
            any::<PropIndex>().prop_map(VecPopBack),
            (any::<PropIndex>(), any::<u64>()).prop_map(VecUnpack),
            any::<PropIndex>().prop_map(VecSwap),
            (any::<PropIndex>(), any::<u16>()).prop_map(|(idx, tag)| PackVariant(idx, tag)),
            (any::<PropIndex>(), any::<u16>()).prop_map(|(idx, tag)| UnpackVariant(idx, tag)),
            (any::<PropIndex>(), any::<u16>()).prop_map(|(idx, tag)| UnpackVariantImmRef(idx, tag)),
            (any::<PropIndex>(), any::<u16>()).prop_map(|(idx, tag)| UnpackVariantMutRef(idx, tag)),
            any::<PropIndex>().prop_map(VariantSwitch),
        ]
    }

    fn materialize(
        self,
        state: &mut FnDefnMaterializeState,
        code_len: usize,
        locals_signature: &Signature,
        jump_table_len: usize,
    ) -> Option<Bytecode> {
        let bytecode = match self {
            BytecodeGen::Simple(bytecode) => bytecode,
            BytecodeGen::LdConst(idx) => {
                if state.constant_pool_len == 0 {
                    return None;
                }
                Bytecode::LdConst(ConstantPoolIndex(
                    idx.index(state.constant_pool_len) as TableIndex
                ))
            }
            BytecodeGen::MutBorrowField((def, field)) => {
                let sd_idx = def.index(state.struct_defs.len());
                let field_count = state.struct_def_to_field_count.get(&sd_idx)?;
                if *field_count == 0 {
                    return None;
                }
                let fh_idx = state.field_handles.add_field_handle(FieldHandle {
                    owner: StructDefinitionIndex(sd_idx as TableIndex),
                    field: field.index(*field_count) as TableIndex,
                });

                let struct_handle =
                    &state.datatype_handles[state.struct_defs[sd_idx].struct_handle.0 as usize];
                if struct_handle.type_parameters.is_empty() {
                    Bytecode::MutBorrowField(fh_idx)
                } else {
                    let sig_idx = state
                        .add_signature_from_type_params(struct_handle.type_param_constraints());
                    let fi_idx = state
                        .field_instantiations
                        .add_instantiation(FieldInstantiation {
                            handle: fh_idx,
                            type_parameters: sig_idx,
                        });
                    Bytecode::MutBorrowFieldGeneric(FieldInstantiationIndex(fi_idx))
                }
            }
            BytecodeGen::ImmBorrowField((def, field)) => {
                let sd_idx = def.index(state.struct_defs.len());
                let field_count = state.struct_def_to_field_count.get(&sd_idx)?;
                if *field_count == 0 {
                    return None;
                }
                let fh_idx = state.field_handles.add_field_handle(FieldHandle {
                    owner: StructDefinitionIndex(sd_idx as TableIndex),
                    field: field.index(*field_count) as TableIndex,
                });

                let struct_handle =
                    &state.datatype_handles[state.struct_defs[sd_idx].struct_handle.0 as usize];
                if struct_handle.type_parameters.is_empty() {
                    Bytecode::ImmBorrowField(fh_idx)
                } else {
                    let sig_idx = state
                        .add_signature_from_type_params(struct_handle.type_param_constraints());
                    let fi_idx = state
                        .field_instantiations
                        .add_instantiation(FieldInstantiation {
                            handle: fh_idx,
                            type_parameters: sig_idx,
                        });
                    Bytecode::ImmBorrowFieldGeneric(FieldInstantiationIndex(fi_idx))
                }
            }
            BytecodeGen::Call(idx) => {
                let func_handles_len = state.function_handles.len();
                let fh_idx = idx.index(func_handles_len);

                if state.function_handles[fh_idx].type_parameters.is_empty() {
                    Bytecode::Call(FunctionHandleIndex(fh_idx as TableIndex))
                } else {
                    Bytecode::CallGeneric(state.get_function_instantiation(fh_idx))
                }
            }
            BytecodeGen::Pack(idx) => {
                let struct_defs_len = state.struct_defs.len();
                let sd_idx = idx.index(struct_defs_len);

                let sd = &state.struct_defs[sd_idx];
                if state.datatype_handles[sd.struct_handle.0 as usize]
                    .type_parameters
                    .is_empty()
                {
                    Bytecode::Pack(StructDefinitionIndex(sd_idx as TableIndex))
                } else {
                    Bytecode::PackGeneric(state.get_type_instantiation(sd_idx))
                }
            }
            BytecodeGen::Unpack(idx) => {
                let struct_defs_len = state.struct_defs.len();
                let sd_idx = idx.index(struct_defs_len);

                let sd = &state.struct_defs[sd_idx];
                if state.datatype_handles[sd.struct_handle.0 as usize]
                    .type_parameters
                    .is_empty()
                {
                    Bytecode::Unpack(StructDefinitionIndex(sd_idx as TableIndex))
                } else {
                    Bytecode::UnpackGeneric(state.get_type_instantiation(sd_idx))
                }
            }
            BytecodeGen::Exists(idx) => {
                let struct_defs_len = state.struct_defs.len();
                let sd_idx = idx.index(struct_defs_len);

                let sd = &state.struct_defs[sd_idx];
                if state.datatype_handles[sd.struct_handle.0 as usize]
                    .type_parameters
                    .is_empty()
                {
                    Bytecode::Exists(StructDefinitionIndex(sd_idx as TableIndex))
                } else {
                    Bytecode::ExistsGeneric(state.get_type_instantiation(sd_idx))
                }
            }
            BytecodeGen::ImmBorrowGlobal(idx) => {
                let struct_defs_len = state.struct_defs.len();
                let sd_idx = idx.index(struct_defs_len);

                let sd = &state.struct_defs[sd_idx];
                if state.datatype_handles[sd.struct_handle.0 as usize]
                    .type_parameters
                    .is_empty()
                {
                    Bytecode::ImmBorrowGlobal(StructDefinitionIndex(sd_idx as TableIndex))
                } else {
                    Bytecode::ImmBorrowGlobalGeneric(state.get_type_instantiation(sd_idx))
                }
            }
            BytecodeGen::MutBorrowGlobal(idx) => {
                let struct_defs_len = state.struct_defs.len();
                let sd_idx = idx.index(struct_defs_len);

                let sd = &state.struct_defs[sd_idx];
                if state.datatype_handles[sd.struct_handle.0 as usize]
                    .type_parameters
                    .is_empty()
                {
                    Bytecode::MutBorrowGlobal(StructDefinitionIndex(sd_idx as TableIndex))
                } else {
                    Bytecode::MutBorrowGlobalGeneric(state.get_type_instantiation(sd_idx))
                }
            }
            BytecodeGen::MoveFrom(idx) => {
                let struct_defs_len = state.struct_defs.len();
                let sd_idx = idx.index(struct_defs_len);

                let sd = &state.struct_defs[sd_idx];
                if state.datatype_handles[sd.struct_handle.0 as usize]
                    .type_parameters
                    .is_empty()
                {
                    Bytecode::MoveFrom(StructDefinitionIndex(sd_idx as TableIndex))
                } else {
                    Bytecode::MoveFromGeneric(state.get_type_instantiation(sd_idx))
                }
            }
            BytecodeGen::MoveTo(idx) => {
                let struct_defs_len = state.struct_defs.len();
                let sd_idx = idx.index(struct_defs_len);

                let sd = &state.struct_defs[sd_idx];
                if state.datatype_handles[sd.struct_handle.0 as usize]
                    .type_parameters
                    .is_empty()
                {
                    Bytecode::MoveTo(StructDefinitionIndex(sd_idx as TableIndex))
                } else {
                    Bytecode::MoveToGeneric(state.get_type_instantiation(sd_idx))
                }
            }
            BytecodeGen::BrTrue(idx) => {
                if code_len == 0 {
                    return None;
                }
                Bytecode::BrTrue(idx.index(code_len) as CodeOffset)
            }
            BytecodeGen::BrFalse(idx) => {
                if code_len == 0 {
                    return None;
                }
                Bytecode::BrFalse(idx.index(code_len) as CodeOffset)
            }
            BytecodeGen::Branch(idx) => {
                if code_len == 0 {
                    return None;
                }
                Bytecode::Branch(idx.index(code_len) as CodeOffset)
            }
            BytecodeGen::CopyLoc(idx) => {
                if locals_signature.is_empty() {
                    return None;
                }
                Bytecode::CopyLoc(idx.index(locals_signature.len()) as LocalIndex)
            }
            BytecodeGen::MoveLoc(idx) => {
                if locals_signature.is_empty() {
                    return None;
                }
                Bytecode::MoveLoc(idx.index(locals_signature.len()) as LocalIndex)
            }
            BytecodeGen::StLoc(idx) => {
                if locals_signature.is_empty() {
                    return None;
                }
                Bytecode::StLoc(idx.index(locals_signature.len()) as LocalIndex)
            }
            BytecodeGen::MutBorrowLoc(idx) => {
                if locals_signature.is_empty() {
                    return None;
                }
                Bytecode::MutBorrowLoc(idx.index(locals_signature.len()) as LocalIndex)
            }
            BytecodeGen::ImmBorrowLoc(idx) => {
                if locals_signature.is_empty() {
                    return None;
                }
                Bytecode::ImmBorrowLoc(idx.index(locals_signature.len()) as LocalIndex)
            }
            BytecodeGen::VecPack((idx, num)) => {
                if num > u16::MAX as u64 {
                    return None;
                }
                let sigs_len = state.signatures.signatures.len();
                if sigs_len == 0 {
                    return None;
                }
                let sig_idx = idx.index(sigs_len);
                let sig = &state.signatures.signatures[sig_idx];
                if !BytecodeGen::is_valid_vector_element_sig(sig) {
                    return None;
                }
                Bytecode::VecPack(SignatureIndex(sig_idx as TableIndex), num)
            }
            BytecodeGen::VecLen(idx) => {
                let sigs_len = state.signatures.signatures.len();
                if sigs_len == 0 {
                    return None;
                }
                let sig_idx = idx.index(sigs_len);
                let sig = &state.signatures.signatures[sig_idx];
                if !BytecodeGen::is_valid_vector_element_sig(sig) {
                    return None;
                }
                Bytecode::VecLen(SignatureIndex(sig_idx as TableIndex))
            }
            BytecodeGen::VecImmBorrow(idx) => {
                let sigs_len = state.signatures.signatures.len();
                if sigs_len == 0 {
                    return None;
                }
                let sig_idx = idx.index(sigs_len);
                let sig = &state.signatures.signatures[sig_idx];
                if !BytecodeGen::is_valid_vector_element_sig(sig) {
                    return None;
                }
                Bytecode::VecImmBorrow(SignatureIndex(sig_idx as TableIndex))
            }
            BytecodeGen::VecMutBorrow(idx) => {
                let sigs_len = state.signatures.signatures.len();
                if sigs_len == 0 {
                    return None;
                }
                let sig_idx = idx.index(sigs_len);
                let sig = &state.signatures.signatures[sig_idx];
                if !BytecodeGen::is_valid_vector_element_sig(sig) {
                    return None;
                }
                Bytecode::VecMutBorrow(SignatureIndex(sig_idx as TableIndex))
            }
            BytecodeGen::VecPushBack(idx) => {
                let sigs_len = state.signatures.signatures.len();
                if sigs_len == 0 {
                    return None;
                }
                let sig_idx = idx.index(sigs_len);
                let sig = &state.signatures.signatures[sig_idx];
                if !BytecodeGen::is_valid_vector_element_sig(sig) {
                    return None;
                }
                Bytecode::VecPushBack(SignatureIndex(sig_idx as TableIndex))
            }
            BytecodeGen::VecPopBack(idx) => {
                let sigs_len = state.signatures.signatures.len();
                if sigs_len == 0 {
                    return None;
                }
                let sig_idx = idx.index(sigs_len);
                let sig = &state.signatures.signatures[sig_idx];
                if !BytecodeGen::is_valid_vector_element_sig(sig) {
                    return None;
                }
                Bytecode::VecPopBack(SignatureIndex(sig_idx as TableIndex))
            }
            BytecodeGen::VecUnpack((idx, num)) => {
                if num > u16::MAX as u64 {
                    return None;
                }
                let sigs_len = state.signatures.signatures.len();
                if sigs_len == 0 {
                    return None;
                }
                let sig_idx = idx.index(sigs_len);
                let sig = &state.signatures.signatures[sig_idx];
                if !BytecodeGen::is_valid_vector_element_sig(sig) {
                    return None;
                }
                Bytecode::VecUnpack(SignatureIndex(sig_idx as TableIndex), num)
            }
            BytecodeGen::VecSwap(idx) => {
                let sigs_len = state.signatures.signatures.len();
                if sigs_len == 0 {
                    return None;
                }
                let sig_idx = idx.index(sigs_len);
                let sig = &state.signatures.signatures[sig_idx];
                if !BytecodeGen::is_valid_vector_element_sig(sig) {
                    return None;
                }
                Bytecode::VecSwap(SignatureIndex(sig_idx as TableIndex))
            }
            BytecodeGen::PackVariant(idx, tag) => {
                let enum_defs_len = state.enum_defs.len();
                if tag as u64 >= VARIANT_COUNT_MAX || enum_defs_len == 0 {
                    return None;
                }
                let ed_idx = idx.index(enum_defs_len);
                let ed = &state.enum_defs[ed_idx];
                if ed.variants.len() <= tag as usize {
                    return None;
                }
                if state.datatype_handles[ed.enum_handle.0 as usize]
                    .type_parameters
                    .is_empty()
                {
                    Bytecode::PackVariant(EnumDefinitionIndex(ed_idx as TableIndex), tag)
                } else {
                    Bytecode::PackVariantGeneric(state.get_enum_type_instantiation(ed_idx), tag)
                }
            }
            BytecodeGen::UnpackVariant(idx, tag) => {
                let enum_defs_len = state.enum_defs.len();
                if tag as u64 >= VARIANT_COUNT_MAX || enum_defs_len == 0 {
                    return None;
                }
                let ed_idx = idx.index(enum_defs_len);

                let ed = &state.enum_defs[ed_idx];
                if ed.variants.len() <= tag as usize {
                    return None;
                }
                if state.datatype_handles[ed.enum_handle.0 as usize]
                    .type_parameters
                    .is_empty()
                {
                    Bytecode::UnpackVariant(EnumDefinitionIndex(ed_idx as TableIndex), tag)
                } else {
                    Bytecode::UnpackVariantGeneric(state.get_enum_type_instantiation(ed_idx), tag)
                }
            }
            BytecodeGen::UnpackVariantImmRef(idx, tag) => {
                let enum_defs_len = state.enum_defs.len();
                if tag as u64 >= VARIANT_COUNT_MAX || enum_defs_len == 0 {
                    return None;
                }
                let ed_idx = idx.index(enum_defs_len);

                let ed = &state.enum_defs[ed_idx];
                if ed.variants.len() <= tag as usize {
                    return None;
                }
                if state.datatype_handles[ed.enum_handle.0 as usize]
                    .type_parameters
                    .is_empty()
                {
                    Bytecode::UnpackVariantImmRef(EnumDefinitionIndex(ed_idx as TableIndex), tag)
                } else {
                    Bytecode::UnpackVariantGenericImmRef(
                        state.get_enum_type_instantiation(ed_idx),
                        tag,
                    )
                }
            }
            BytecodeGen::UnpackVariantMutRef(idx, tag) => {
                let enum_defs_len = state.enum_defs.len();
                if tag as u64 >= VARIANT_COUNT_MAX || enum_defs_len == 0 {
                    return None;
                }
                let ed_idx = idx.index(enum_defs_len);
                let ed = &state.enum_defs[ed_idx];
                if ed.variants.len() <= tag as usize {
                    return None;
                }
                if state.datatype_handles[ed.enum_handle.0 as usize]
                    .type_parameters
                    .is_empty()
                {
                    Bytecode::UnpackVariantMutRef(EnumDefinitionIndex(ed_idx as TableIndex), tag)
                } else {
                    Bytecode::UnpackVariantGenericMutRef(
                        state.get_enum_type_instantiation(ed_idx),
                        tag,
                    )
                }
            }
            BytecodeGen::VariantSwitch(idx) => {
                if jump_table_len == 0 {
                    return None;
                }
                Bytecode::VariantSwitch(VariantJumpTableIndex(
                    idx.index(jump_table_len) as TableIndex
                ))
            }
        };

        Some(bytecode)
    }

    /// Checks if the given type is well defined in the given context.
    /// No references are permitted.
    fn check_signature_token(token: &SignatureToken) -> bool {
        use SignatureToken::*;
        match token {
            U8 | U16 | U32 | U64 | U128 | U256 | Bool | Address | Signer | Datatype(_)
            | TypeParameter(_) => true,
            Vector(element_token) => BytecodeGen::check_signature_token(element_token),
            DatatypeInstantiation(_, type_arguments) => type_arguments
                .iter()
                .all(BytecodeGen::check_signature_token),
            Reference(_) | MutableReference(_) => false,
        }
    }

    fn is_valid_vector_element_sig(sig: &Signature) -> bool {
        if sig.len() != 1 {
            return false;
        }
        BytecodeGen::check_signature_token(&sig.0[0])
    }

    fn simple_bytecode_strategy() -> impl Strategy<Value = Bytecode> {
        prop_oneof![
        // The numbers are relative weights, somewhat arbitrarily picked.
        9 => Self::just_bytecode_strategy(),
        1 => any::<u64>().prop_map(Bytecode::LdU64),
        1 => any::<u8>().prop_map(Bytecode::LdU8),
        1 => any::<u128>().prop_map(Bytecode::LdU128),
        1 => any::<u16>().prop_map(Bytecode::LdU16),
        1 => any::<u32>().prop_map(Bytecode::LdU32),
        1 => any::<U256>().prop_map(Bytecode::LdU256),
        ]
    }

    fn just_bytecode_strategy() -> impl Strategy<Value = Bytecode> {
        use Bytecode::*;

        static JUST_BYTECODES: &[Bytecode] = &[
            FreezeRef, Pop, Ret, LdTrue, LdFalse, ReadRef, WriteRef, Add, Sub, Mul, Mod, Div,
            BitOr, BitAnd, Xor, Or, And, Eq, Neq, Lt, Gt, Le, Ge, Abort, CastU8, CastU64, CastU128,
            CastU16, CastU32, CastU256, Not, Nop, Shl, Shr,
        ];
        select(JUST_BYTECODES)
    }
}
