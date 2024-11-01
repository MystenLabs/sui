// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::{
    errors::PartialVMResult,
    file_format::{AbilitySet, DatatypeHandleIndex, SignatureToken},
    safe_unwrap, CompiledModule,
};
use move_bytecode_verifier_meter::{Meter, Scope};
use std::{
    cmp::max,
    collections::{btree_map::Entry, BTreeMap},
};

const TYPE_ARG_COST: u128 = 1;

pub struct AbilityCache<'a> {
    module: &'a CompiledModule,
    vector_results: BTreeMap<AbilitySet, AbilitySet>,
    datatype_results: BTreeMap<DatatypeHandleIndex, BTreeMap<Vec<AbilitySet>, AbilitySet>>,
}

impl<'a> AbilityCache<'a> {
    pub fn new(module: &'a CompiledModule) -> Self {
        Self {
            module,
            vector_results: BTreeMap::new(),
            datatype_results: BTreeMap::new(),
        }
    }

    pub fn abilities(
        &mut self,
        scope: Scope,
        meter: &mut (impl Meter + ?Sized),
        type_parameter_abilities: &[AbilitySet],
        ty: &SignatureToken,
    ) -> PartialVMResult<AbilitySet> {
        use SignatureToken as S;

        Ok(match ty {
            S::Bool | S::U8 | S::U16 | S::U32 | S::U64 | S::U128 | S::U256 | S::Address => {
                AbilitySet::PRIMITIVES
            }

            S::Reference(_) | S::MutableReference(_) => AbilitySet::REFERENCES,
            S::Signer => AbilitySet::SIGNER,
            S::TypeParameter(idx) => *safe_unwrap!(type_parameter_abilities.get(*idx as usize)),
            S::Datatype(idx) => {
                let sh = self.module.datatype_handle_at(*idx);
                sh.abilities
            }
            S::Vector(inner) => {
                let inner_abilities =
                    self.abilities(scope, meter, type_parameter_abilities, inner)?;
                let entry = self.vector_results.entry(inner_abilities);
                match entry {
                    Entry::Occupied(entry) => *entry.get(),
                    Entry::Vacant(entry) => {
                        meter.add(scope, TYPE_ARG_COST)?;
                        let abilities = AbilitySet::polymorphic_abilities(
                            AbilitySet::VECTOR,
                            vec![false],
                            vec![inner_abilities],
                        )?;
                        entry.insert(abilities);
                        abilities
                    }
                }
            }
            S::DatatypeInstantiation(inst) => {
                let (idx, type_args) = &**inst;
                let type_arg_abilities = type_args
                    .iter()
                    .map(|arg| self.abilities(scope, meter, type_parameter_abilities, arg))
                    .collect::<PartialVMResult<Vec<_>>>()?;
                let entry = self
                    .datatype_results
                    .entry(*idx)
                    .or_default()
                    .entry(type_arg_abilities.clone());
                match entry {
                    Entry::Occupied(entry) => *entry.get(),
                    Entry::Vacant(entry) => {
                        meter.add_items(scope, TYPE_ARG_COST, max(type_args.len(), 1))?;
                        let sh = self.module.datatype_handle_at(*idx);
                        let declared_abilities = sh.abilities;
                        let abilities = AbilitySet::polymorphic_abilities(
                            declared_abilities,
                            sh.type_parameters.iter().map(|param| param.is_phantom),
                            type_arg_abilities,
                        )?;
                        entry.insert(abilities);
                        abilities
                    }
                }
            }
        })
    }
}
