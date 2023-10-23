// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    file_format::{
        AbilitySet, DataTypeHandle, DataTypeHandleIndex, DataTypeTyParameter, EnumDefinition,
        EnumDefinitionIndex, FieldDefinition, IdentifierIndex, ModuleHandleIndex, SignatureToken,
        StructDefinition, StructFieldInformation, TableIndex, TypeSignature, VariantDefinition,
    },
    internals::ModuleIndex,
    proptest_types::{
        prop_index_avoid,
        signature::{AbilitySetGen, SignatureTokenGen},
    },
};
use proptest::{
    collection::{vec, SizeRange},
    option,
    prelude::*,
    sample::Index as PropIndex,
    std_facade::hash_set::HashSet,
};
use std::collections::BTreeSet;

#[derive(Debug)]
struct TypeSignatureIndex(u16);

#[derive(Debug)]
pub struct StDefnMaterializeState {
    pub self_module_handle_idx: ModuleHandleIndex,
    pub identifiers_len: usize,
    pub data_type_handles: Vec<DataTypeHandle>,
    pub new_handles: BTreeSet<(ModuleHandleIndex, IdentifierIndex)>,
}

impl StDefnMaterializeState {
    pub fn new(
        self_module_handle_idx: ModuleHandleIndex,
        identifiers_len: usize,
        data_type_handles: Vec<DataTypeHandle>,
    ) -> Self {
        Self {
            self_module_handle_idx,
            identifiers_len,
            data_type_handles,
            new_handles: BTreeSet::new(),
        }
    }

    fn add_data_type_handle(&mut self, handle: DataTypeHandle) -> Option<DataTypeHandleIndex> {
        if self.new_handles.insert((handle.module, handle.name)) {
            self.data_type_handles.push(handle);
            Some(DataTypeHandleIndex(
                (self.data_type_handles.len() - 1) as u16,
            ))
        } else {
            None
        }
    }

    fn potential_abilities(&self, ty: &SignatureToken) -> AbilitySet {
        use SignatureToken::*;

        match ty {
            Bool | U8 | U16 | U32 | U64 | U128 | U256 | Address => AbilitySet::PRIMITIVES,

            Reference(_) | MutableReference(_) => AbilitySet::REFERENCES,
            Signer => AbilitySet::SIGNER,
            TypeParameter(_) => AbilitySet::ALL,
            Vector(ty) => {
                let inner = self.potential_abilities(ty);
                inner.intersect(AbilitySet::VECTOR)
            }
            DataType(idx) => {
                let sh = &self.data_type_handles[idx.0 as usize];
                sh.abilities
            }
            DataTypeInstantiation(idx, type_args) => {
                let sh = &self.data_type_handles[idx.0 as usize];

                // Gather the abilities of the type actuals.
                let type_args_abilities = type_args.iter().map(|ty| self.potential_abilities(ty));
                type_args_abilities.fold(sh.abilities, |acc, ty_arg_abilities| {
                    acc.intersect(ty_arg_abilities)
                })
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct DataTypeHandleGen {
    module_idx: PropIndex,
    name_idx: PropIndex,
    abilities: AbilitySetGen,
    type_parameters: Vec<(AbilitySetGen, bool)>,
}

impl DataTypeHandleGen {
    pub fn strategy(ability_count: impl Into<SizeRange>) -> impl Strategy<Value = Self> {
        let ability_count = ability_count.into();
        (
            any::<PropIndex>(),
            any::<PropIndex>(),
            AbilitySetGen::strategy(),
            vec((AbilitySetGen::strategy(), any::<bool>()), ability_count),
        )
            .prop_map(|(module_idx, name_idx, abilities, type_parameters)| Self {
                module_idx,
                name_idx,
                abilities,
                type_parameters,
            })
    }

    pub fn materialize(
        self,
        self_module_handle_idx: ModuleHandleIndex,
        module_len: usize,
        identifiers_len: usize,
    ) -> DataTypeHandle {
        let idx = prop_index_avoid(
            self.module_idx,
            self_module_handle_idx.into_index(),
            module_len,
        );
        let type_parameters = self
            .type_parameters
            .into_iter()
            .map(|(constraints, is_phantom)| DataTypeTyParameter {
                constraints: constraints.materialize(),
                is_phantom,
            })
            .collect();
        DataTypeHandle {
            module: ModuleHandleIndex(idx as TableIndex),
            name: IdentifierIndex(self.name_idx.index(identifiers_len) as TableIndex),
            abilities: self.abilities.materialize(),
            type_parameters,
        }
    }
}

#[derive(Clone, Debug)]
pub struct StructDefinitionGen {
    name_idx: PropIndex,
    abilities: AbilitySetGen,
    type_parameters: Vec<(AbilitySetGen, bool)>,
    #[allow(dead_code)]
    is_public: bool,
    field_defs: Option<Vec<FieldDefinitionGen>>,
}

impl StructDefinitionGen {
    pub fn strategy(
        field_count: impl Into<SizeRange>,
        type_parameter_count: impl Into<SizeRange>,
    ) -> impl Strategy<Value = Self> {
        (
            any::<PropIndex>(),
            AbilitySetGen::strategy(),
            vec(
                (AbilitySetGen::strategy(), any::<bool>()),
                type_parameter_count,
            ),
            any::<bool>(),
            option::of(vec(FieldDefinitionGen::strategy(), field_count)),
        )
            .prop_map(
                |(name_idx, abilities, type_parameters, is_public, field_defs)| Self {
                    name_idx,
                    abilities,
                    type_parameters,
                    is_public,
                    field_defs,
                },
            )
    }

    pub fn materialize(
        self,
        state: &mut StDefnMaterializeState,
    ) -> (Option<StructDefinition>, usize) {
        let mut field_names = HashSet::new();
        let mut fields = vec![];
        match self.field_defs {
            None => (),
            Some(field_defs_gen) => {
                for fd_gen in field_defs_gen {
                    let field = fd_gen.materialize(state);
                    if field_names.insert(field.name) {
                        fields.push(field);
                    }
                }
            }
        };
        let abilities = fields
            .iter()
            .fold(self.abilities.materialize(), |acc, field| {
                acc.intersect(state.potential_abilities(&field.signature.0))
            });

        let type_parameters = self
            .type_parameters
            .into_iter()
            .map(|(constraints, is_phantom)| DataTypeTyParameter {
                constraints: constraints.materialize(),
                is_phantom,
            })
            .collect();
        let handle = DataTypeHandle {
            module: state.self_module_handle_idx,
            name: IdentifierIndex(self.name_idx.index(state.identifiers_len) as TableIndex),
            abilities,
            type_parameters,
        };
        match state.add_data_type_handle(handle) {
            Some(struct_handle) => {
                if fields.is_empty() {
                    (
                        Some(StructDefinition {
                            struct_handle,
                            field_information: StructFieldInformation::Native,
                        }),
                        0,
                    )
                } else {
                    let field_count = fields.len();
                    let field_information = StructFieldInformation::Declared(fields);
                    (
                        Some(StructDefinition {
                            struct_handle,
                            field_information,
                        }),
                        field_count,
                    )
                }
            }
            None => (None, 0),
        }
    }
}

#[derive(Clone, Debug)]
pub struct EnumDefinitionGen {
    name_idx: PropIndex,
    abilities: AbilitySetGen,
    type_parameters: Vec<(AbilitySetGen, bool)>,
    #[allow(dead_code)]
    is_public: bool,
    variant_defs: Vec<VariantDefinitionGen>,
}

impl EnumDefinitionGen {
    pub fn strategy(
        variant_count: impl Into<SizeRange>,
        field_count: impl Into<SizeRange>,
        type_parameter_count: impl Into<SizeRange>,
    ) -> impl Strategy<Value = Self> {
        (
            any::<PropIndex>(),
            AbilitySetGen::strategy(),
            vec(
                (AbilitySetGen::strategy(), any::<bool>()),
                type_parameter_count,
            ),
            any::<bool>(),
            vec(VariantDefinitionGen::strategy(field_count), variant_count),
        )
            .prop_map(
                |(name_idx, abilities, type_parameters, is_public, variant_defs)| Self {
                    name_idx,
                    abilities,
                    type_parameters,
                    is_public,
                    variant_defs,
                },
            )
    }

    pub fn materialize(
        self,
        state: &mut StDefnMaterializeState,
        index: usize,
    ) -> Option<EnumDefinition> {
        let mut variant_names = HashSet::new();
        let mut variants = vec![];
        let enum_idx = EnumDefinitionIndex(index as TableIndex);
        for vd_gen in self.variant_defs {
            let variant = vd_gen.materialize(state, enum_idx);
            if variant_names.insert(variant.variant_name) {
                variants.push(variant);
            }
        }
        let abilities = variants
            .iter()
            .flat_map(|variant| variant.fields.iter())
            .fold(self.abilities.materialize(), |acc, field| {
                acc.intersect(state.potential_abilities(&field.signature.0))
            });

        let type_parameters = self
            .type_parameters
            .into_iter()
            .map(|(constraints, is_phantom)| DataTypeTyParameter {
                constraints: constraints.materialize(),
                is_phantom,
            })
            .collect();
        let handle = DataTypeHandle {
            module: state.self_module_handle_idx,
            name: IdentifierIndex(self.name_idx.index(state.identifiers_len) as TableIndex),
            abilities,
            type_parameters,
        };
        match state.add_data_type_handle(handle) {
            Some(enum_handle) => Some(EnumDefinition {
                enum_handle,
                variants,
            }),
            None => None,
        }
    }
}

#[derive(Clone, Debug)]
struct VariantDefinitionGen {
    name_idx: PropIndex,
    signature_gen: Vec<FieldDefinitionGen>,
}

impl VariantDefinitionGen {
    fn strategy(field_count: impl Into<SizeRange>) -> impl Strategy<Value = Self> {
        (
            any::<PropIndex>(),
            vec(FieldDefinitionGen::strategy(), field_count),
        )
            .prop_map(|(name_idx, signature_gen)| Self {
                name_idx,
                signature_gen,
            })
    }

    fn materialize(
        self,
        state: &StDefnMaterializeState,
        enum_def: EnumDefinitionIndex,
    ) -> VariantDefinition {
        VariantDefinition {
            variant_name: IdentifierIndex(self.name_idx.index(state.identifiers_len) as TableIndex),
            enum_def,
            fields: self
                .signature_gen
                .into_iter()
                .map(|field| field.materialize(state))
                .collect(),
        }
    }
}

#[derive(Clone, Debug)]
struct FieldDefinitionGen {
    name_idx: PropIndex,
    signature_gen: SignatureTokenGen,
}

impl FieldDefinitionGen {
    fn strategy() -> impl Strategy<Value = Self> {
        (any::<PropIndex>(), SignatureTokenGen::atom_strategy()).prop_map(
            |(name_idx, signature_gen)| Self {
                name_idx,
                signature_gen,
            },
        )
    }

    fn materialize(self, state: &StDefnMaterializeState) -> FieldDefinition {
        FieldDefinition {
            name: IdentifierIndex(self.name_idx.index(state.identifiers_len) as TableIndex),
            signature: TypeSignature(self.signature_gen.materialize(&state.data_type_handles)),
        }
    }
}
