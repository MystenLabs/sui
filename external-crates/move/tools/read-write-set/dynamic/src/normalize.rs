// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::dynamic_analysis::{
    bind_formals, concretize, ConcretizedFormals, ConcretizedSecondaryIndexes,
};
use anyhow::{anyhow, bail, Result};
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::{
    account_address::AccountAddress,
    identifier::{IdentStr, Identifier},
    language_storage::{ModuleId, ResourceKey, TypeTag},
    resolver::MoveResolver,
};
use move_read_write_set_types::ReadWriteSet;
use std::collections::BTreeMap;

pub struct NormalizedReadWriteSetAnalysis(BTreeMap<ModuleId, BTreeMap<Identifier, ReadWriteSet>>);

impl NormalizedReadWriteSetAnalysis {
    pub fn new(inner: BTreeMap<ModuleId, BTreeMap<Identifier, ReadWriteSet>>) -> Self {
        Self(inner)
    }

    /// Trim the analysis result by dropping all the results in the trie. This should not affect the
    /// correctness for those non secondary index access as all relevant info should be stored in
    /// the root node already. Trimming the result will yield a higher inference speed at runtime.
    pub fn trim(&self) -> Self {
        Self(
            self.0
                .iter()
                .map(|(module_id, func_map)| {
                    (
                        module_id.clone(),
                        func_map
                            .iter()
                            .map(|(func_name, analysis_result)| {
                                (func_name.clone(), analysis_result.trim())
                            })
                            .collect(),
                    )
                })
                .collect(),
        )
    }

    fn get_summary(&self, module: &ModuleId, fun: &IdentStr) -> Option<&ReadWriteSet> {
        self.0.get(module)?.get(fun)
    }

    /// Returns an overapproximation of the `ResourceKey`'s in global storage that will be written
    /// by `module::fun` if called with arguments `signers`, `actuals`, `type_actuals` in state
    /// `blockchain_view`.
    pub fn get_keys_written(
        &self,
        module: &ModuleId,
        fun: &IdentStr,
        signers: &[AccountAddress],
        actuals: &[Vec<u8>],
        type_actuals: &[TypeTag],
        blockchain_view: &impl MoveResolver,
    ) -> Result<Vec<ResourceKey>> {
        self.get_concretized_keys(
            module,
            fun,
            signers,
            actuals,
            type_actuals,
            blockchain_view,
            true,
        )
    }

    /// Returns an overapproximation of the `ResourceKey`'s in global storage that will be read by
    /// `module::fun` if called with arguments `signers`, `actuals`, `type_actuals` in state
    /// `blockchain_view`.
    pub fn get_keys_read(
        &self,
        module: &ModuleId,
        fun: &IdentStr,
        signers: &[AccountAddress],
        actuals: &[Vec<u8>],
        type_actuals: &[TypeTag],
        blockchain_view: &impl MoveResolver,
    ) -> Result<Vec<ResourceKey>> {
        self.get_concretized_keys(
            module,
            fun,
            signers,
            actuals,
            type_actuals,
            blockchain_view,
            false,
        )
    }

    /// Returns an overapproximation of the `ResourceKey`'s in global storage that will be accesses
    /// by module::fun` if called with arguments `signers`, `actuals`, `type_actuals` in state
    /// `blockchain_view`.
    /// If `is_write` is true, only ResourceKey's written will be returned; otherwise, only
    /// ResourceKey's read will be returned.
    pub fn get_concretized_keys(
        &self,
        module: &ModuleId,
        fun: &IdentStr,
        signers: &[AccountAddress],
        actuals: &[Vec<u8>],
        type_actuals: &[TypeTag],
        blockchain_view: &impl MoveResolver,
        is_write: bool,
    ) -> Result<Vec<ResourceKey>> {
        if let Some(state) = self.get_summary(module, fun) {
            let results = concretize(
                state,
                module,
                fun,
                signers,
                actuals,
                type_actuals,
                blockchain_view,
            )?;
            Ok(if is_write {
                results
                    .get_keys_written()
                    .ok_or_else(|| anyhow!("Failed to get keys written"))?
            } else {
                results
                    .get_keys_read()
                    .ok_or_else(|| anyhow!("Failed to get keys read"))?
            })
        } else {
            bail!("Couldn't resolve function {:?}::{:?}", module, fun)
        }
    }

    /// Returns an overapproximation of the access paths in global storage that will be read/written
    /// by `module::fun` if called with arguments `signers`, `actuals`, `type_actuals` in state
    /// `blockchain_view`.
    pub fn get_concretized_summary(
        &self,
        module: &ModuleId,
        fun: &IdentStr,
        signers: &[AccountAddress],
        actuals: &[Vec<u8>],
        type_actuals: &[TypeTag],
        blockchain_view: &impl MoveResolver,
    ) -> Result<ConcretizedSecondaryIndexes> {
        let state = self
            .get_summary(module, fun)
            .ok_or_else(|| anyhow!("Function {}::{} to found", module, fun))?;
        concretize(
            state,
            module,
            fun,
            signers,
            actuals,
            type_actuals,
            blockchain_view,
        )
    }

    pub fn get_canonical_summary(&self, module: &ModuleId, fun: &IdentStr) -> Option<ReadWriteSet> {
        self.get_summary(module, fun).cloned()
    }

    /// Returns the access paths in global storage that will be read/written by `module::fun` if called with arguments `signers`, `actuals`, `type_actuals`. This will be an overapproximation if `module::fun` contains no secondary indexes; otherwise it is neither an overapproximation nor an underapproximation
    /// by `module::fun` if called with arguments `signers`, `actuals`, `type_actuals`.
    ///
    /// We say "partially concretized" because the summary may contain secondary indexes that require reads from the current blockchain state to be concretized. If desired, the caller can concretized them using <add API for this>
    /// be resolved or not.
    pub fn get_partially_concretized_summary<R: GetModule>(
        &self,
        module: &ModuleId,
        fun: &IdentStr,
        signers: &[AccountAddress],
        actuals: &[Vec<u8>],
        type_actuals: &[TypeTag],
        module_cache: &R,
    ) -> Result<ConcretizedFormals> {
        let state = self
            .get_summary(module, fun)
            .ok_or_else(|| anyhow!("Function {}::{} not found", module, fun))?;
        bind_formals(
            state,
            module,
            fun,
            signers,
            actuals,
            type_actuals,
            module_cache,
        )
    }

    /// Return `true` if `module`::`fun` may read an address from the blockchain state and
    /// subsequently read/write a resource stored at that address. Return `false` if the function
    /// will not do this in any possible concrete execution. Return an error if `module`::`fun` does
    /// not exist.
    pub fn may_have_secondary_indexes(&self, module: &ModuleId, fun: &IdentStr) -> Result<bool> {
        let state = self
            .get_summary(module, fun)
            .ok_or_else(|| anyhow!("Function {}::{} to found", module, fun))?;
        let mut has_secondary_index = false;
        state.iter_paths(|offset, _| {
            if offset.has_secondary_index() {
                has_secondary_index = true;
            }
            Some(())
        });
        Ok(has_secondary_index)
    }

    pub fn into_inner(self) -> BTreeMap<ModuleId, BTreeMap<Identifier, ReadWriteSet>> {
        self.0
    }
}
