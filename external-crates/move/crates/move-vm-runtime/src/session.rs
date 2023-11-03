// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    data_cache::TransactionDataCache, native_extensions::NativeContextExtensions,
    runtime::VMRuntime,
};
use move_binary_format::{
    errors::*,
    file_format::{AbilitySet, LocalIndex},
};
use move_core_types::{
    account_address::AccountAddress,
    annotated_value as A,
    effects::{ChangeSet, Event},
    identifier::IdentStr,
    language_storage::{ModuleId, TypeTag},
    resolver::MoveResolver,
    runtime_value::MoveTypeLayout,
};
#[cfg(feature = "gas-profiler")]
use move_vm_profiler::GasProfiler;
use move_vm_types::{
    data_store::DataStore,
    gas::GasMeter,
    loaded_data::runtime_types::{CachedStructIndex, StructType, Type},
};
use std::{borrow::Borrow, sync::Arc};

pub struct Session<'r, 'l, S> {
    pub(crate) runtime: &'l VMRuntime,
    pub(crate) data_cache: TransactionDataCache<'l, S>,
    pub(crate) native_extensions: NativeContextExtensions<'r>,
}

/// Serialized return values from function/script execution
/// Simple struct is designed just to convey meaning behind serialized values
#[derive(Debug)]
pub struct SerializedReturnValues {
    /// The value of any arguments that were mutably borrowed.
    /// Non-mut borrowed values are not included
    pub mutable_reference_outputs: Vec<(LocalIndex, Vec<u8>, MoveTypeLayout)>,
    /// The return values from the function
    pub return_values: Vec<(Vec<u8>, MoveTypeLayout)>,
}

impl<'r, 'l, S: MoveResolver> Session<'r, 'l, S> {
    /// Execute a Move function with the given arguments. This is mainly designed for an external
    /// environment to invoke system logic written in Move.
    ///
    /// NOTE: There are NO checks on the `args` except that they can deserialize into the provided
    /// types.
    /// The ability to deserialize `args` into arbitrary types is *very* powerful, e.g. it can
    /// used to manufacture `signer`'s or `Coin`'s from raw bytes. It is the responsibility of the
    /// caller (e.g. adapter) to ensure that this power is used responsibly/securely for its
    /// use-case.
    ///
    /// The caller MUST ensure
    ///   - All types and modules referred to by the type arguments exist.
    ///   - The signature is valid for the rules of the adapter
    ///
    /// The Move VM MUST return an invariant violation if the caller fails to follow any of the
    /// rules above.
    ///
    /// The VM will check that the function is marked as an 'entry' function.
    ///
    /// Currently if any other error occurs during execution, the Move VM will simply propagate that
    /// error back to the outer environment without handling/translating it. This behavior may be
    /// revised in the future.
    ///
    /// In case an invariant violation occurs, the whole Session should be considered corrupted and
    /// one shall not proceed with effect generation.
    pub fn execute_entry_function(
        &mut self,
        module: &ModuleId,
        function_name: &IdentStr,
        ty_args: Vec<Type>,
        args: Vec<impl Borrow<[u8]>>,
        gas_meter: &mut impl GasMeter,
    ) -> VMResult<SerializedReturnValues> {
        let bypass_declared_entry_check = false;
        self.runtime.execute_function(
            module,
            function_name,
            ty_args,
            args,
            &mut self.data_cache,
            gas_meter,
            &mut self.native_extensions,
            bypass_declared_entry_check,
        )
    }

    /// Similar to execute_entry_function, but it bypasses visibility checks
    pub fn execute_function_bypass_visibility(
        &mut self,
        module: &ModuleId,
        function_name: &IdentStr,
        ty_args: Vec<Type>,
        args: Vec<impl Borrow<[u8]>>,
        gas_meter: &mut impl GasMeter,
    ) -> VMResult<SerializedReturnValues> {
        #[cfg(feature = "gas-profiler")]
        {
            if gas_meter.get_profiler_mut().is_none() {
                gas_meter.set_profiler(GasProfiler::init_default_cfg(
                    function_name.to_string(),
                    gas_meter.remaining_gas().into(),
                ));
            }
        }

        let bypass_declared_entry_check = true;
        self.runtime.execute_function(
            module,
            function_name,
            ty_args,
            args,
            &mut self.data_cache,
            gas_meter,
            &mut self.native_extensions,
            bypass_declared_entry_check,
        )
    }

    /// Publish the given module.
    ///
    /// The Move VM MUST return a user error, i.e., an error that's not an invariant violation, if
    ///   - The module fails to deserialize or verify.
    ///   - The sender address does not match that of the module.
    ///
    /// The Move VM should not be able to produce other user errors.
    /// Besides, no user input should cause the Move VM to return an invariant violation.
    ///
    /// In case an invariant violation occurs, the whole Session should be considered corrupted and
    /// one shall not proceed with effect generation.
    pub fn publish_module(
        &mut self,
        module: Vec<u8>,
        sender: AccountAddress,
        gas_meter: &mut impl GasMeter,
    ) -> VMResult<()> {
        self.publish_module_bundle(vec![module], sender, gas_meter)
    }

    /// Publish a series of modules.
    ///
    /// The Move VM MUST return a user error, i.e., an error that's not an invariant violation, if
    /// any module fails to deserialize or verify (see the full list of  failing conditions in the
    /// `publish_module` API). The publishing of the module series is an all-or-nothing action:
    /// either all modules are published to the data store or none is.
    ///
    /// Similar to the `publish_module` API, the Move VM should not be able to produce other user
    /// errors. Besides, no user input should cause the Move VM to return an invariant violation.
    ///
    /// In case an invariant violation occurs, the whole Session should be considered corrupted and
    /// one shall not proceed with effect generation.
    pub fn publish_module_bundle(
        &mut self,
        modules: Vec<Vec<u8>>,
        sender: AccountAddress,
        gas_meter: &mut impl GasMeter,
    ) -> VMResult<()> {
        self.runtime
            .publish_module_bundle(modules, sender, &mut self.data_cache, gas_meter)
    }

    pub fn num_mutated_accounts(&self, sender: &AccountAddress) -> u64 {
        self.data_cache.num_mutated_accounts(sender)
    }

    /// Finish up the session and produce the side effects.
    ///
    /// This function should always succeed with no user errors returned, barring invariant violations.
    ///
    /// This MUST NOT be called if there is a previous invocation that failed with an invariant violation.
    pub fn finish(self) -> (VMResult<(ChangeSet, Vec<Event>)>, S) {
        let (res, remote) = self.data_cache.into_effects();
        (res.map_err(|e| e.finish(Location::Undefined)), remote)
    }

    pub fn vm_config(&self) -> &move_vm_config::runtime::VMConfig {
        self.runtime.loader().vm_config()
    }

    /// Same like `finish`, but also extracts the native context extensions from the session.
    pub fn finish_with_extensions(
        self,
    ) -> (
        VMResult<(ChangeSet, Vec<Event>, NativeContextExtensions<'r>)>,
        S,
    ) {
        let Session {
            data_cache,
            native_extensions,
            ..
        } = self;
        let (res, remote) = data_cache.into_effects();
        (
            res.map(|(change_set, events)| (change_set, events, native_extensions))
                .map_err(|e| e.finish(Location::Undefined)),
            remote,
        )
    }

    /// Load a module, a function, and all of its types into cache
    pub fn load_function(
        &self,
        module_id: &ModuleId,
        function_name: &IdentStr,
        type_arguments: &[Type],
    ) -> VMResult<LoadedFunctionInstantiation> {
        let (_, _, _, instantiation) = self.runtime.loader().load_function(
            module_id,
            function_name,
            type_arguments,
            &self.data_cache,
        )?;
        Ok(instantiation)
    }

    /// Load a struct by its name to get the global index that it is referenced by within the
    /// loader, and the loaded struct information.  This operation also ensures the defining module
    /// is loaded from the data store and will fail if the type does not exist in that module.
    pub fn load_struct(
        &self,
        module_id: &ModuleId,
        struct_name: &IdentStr,
    ) -> VMResult<(CachedStructIndex, Arc<StructType>)> {
        self.runtime
            .loader()
            .load_struct_by_name(struct_name, module_id, &self.data_cache)
    }

    pub fn load_type(&self, type_tag: &TypeTag) -> VMResult<Type> {
        self.runtime.loader().load_type(type_tag, &self.data_cache)
    }

    pub fn get_type_layout(&self, type_tag: &TypeTag) -> VMResult<MoveTypeLayout> {
        self.runtime
            .loader()
            .get_type_layout(type_tag, &self.data_cache)
    }

    pub fn get_fully_annotated_type_layout(
        &self,
        type_tag: &TypeTag,
    ) -> VMResult<A::MoveTypeLayout> {
        self.runtime
            .loader()
            .get_fully_annotated_type_layout(type_tag, &self.data_cache)
    }

    pub fn type_to_type_layout(&self, ty: &Type) -> VMResult<MoveTypeLayout> {
        self.runtime
            .loader()
            .type_to_type_layout(ty)
            .map_err(|e| e.finish(Location::Undefined))
    }

    pub fn type_to_fully_annotated_layout(&self, ty: &Type) -> VMResult<A::MoveTypeLayout> {
        self.runtime
            .loader()
            .type_to_fully_annotated_layout(ty)
            .map_err(|e| e.finish(Location::Undefined))
    }

    pub fn get_type_tag(&self, ty: &Type) -> VMResult<TypeTag> {
        self.runtime
            .loader()
            .type_to_type_tag(ty)
            .map_err(|e| e.finish(Location::Undefined))
    }

    /// Fetch a struct type from cache, if the index is in bounds
    /// Helpful when paired with load_type, or any other API that returns 'Type'
    pub fn get_struct_type(&self, index: CachedStructIndex) -> Option<Arc<StructType>> {
        self.runtime.loader().get_struct_type(index)
    }

    /// Gets the abilities for this type, at it's particular instantiation
    pub fn get_type_abilities(&self, ty: &Type) -> VMResult<AbilitySet> {
        self.runtime
            .loader()
            .abilities(ty)
            .map_err(|e| e.finish(Location::Undefined))
    }

    /// Gets the remote resolver used by the data store
    pub fn get_resolver(&self) -> &S {
        self.data_cache.get_remote_resolver()
    }

    pub fn get_resolver_mut(&mut self) -> &mut S {
        self.data_cache.get_remote_resolver_mut()
    }

    /// Gets the underlying data store
    pub fn get_data_store(&mut self) -> &mut dyn DataStore {
        &mut self.data_cache
    }

    /// Gets the underlying native extensions.
    pub fn get_native_extensions(&mut self) -> &mut NativeContextExtensions<'r> {
        &mut self.native_extensions
    }
}

pub struct LoadedFunctionInstantiation {
    pub parameters: Vec<Type>,
    pub return_: Vec<Type>,
}
