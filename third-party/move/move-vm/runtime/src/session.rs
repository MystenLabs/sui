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
    effects::{ChangeSet, Event},
    identifier::IdentStr,
    language_storage::{ModuleId, TypeTag},
    resolver::MoveResolver,
    value::MoveTypeLayout,
};
use move_vm_types::{
    data_store::DataStore,
    gas::GasMeter,
    loaded_data::runtime_types::{CachedStructIndex, StructType, Type},
};
use std::{borrow::Borrow, sync::Arc};

pub struct Session<'r, 'l, S> {
    pub(crate) runtime: &'l VMRuntime,
    pub(crate) data_cache: TransactionDataCache<'r, 'l, S>,
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
        ty_args: Vec<TypeTag>,
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
        ty_args: Vec<TypeTag>,
        args: Vec<impl Borrow<[u8]>>,
        gas_meter: &mut impl GasMeter,
    ) -> VMResult<SerializedReturnValues> {
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

    /// Execute a transaction script.
    ///
    /// The Move VM MUST return a user error (in other words, an error that's not an invariant
    /// violation) if
    ///   - The script fails to deserialize or verify. Not all expressible signatures are valid.
    ///     See `move_bytecode_verifier::script_signature` for the rules.
    ///   - Type arguments refer to a non-existent type.
    ///   - Arguments (senders included) fail to deserialize or fail to match the signature of the
    ///     script function.
    ///
    /// If any other error occurs during execution, the Move VM MUST propagate that error back to
    /// the caller.
    /// Besides, no user input should cause the Move VM to return an invariant violation.
    ///
    /// In case an invariant violation occurs, the whole Session should be considered corrupted and
    /// one shall not proceed with effect generation.
    pub fn execute_script(
        &mut self,
        script: impl Borrow<[u8]>,
        ty_args: Vec<TypeTag>,
        args: Vec<impl Borrow<[u8]>>,
        gas_meter: &mut impl GasMeter,
    ) -> VMResult<SerializedReturnValues> {
        self.runtime.execute_script(
            script,
            ty_args,
            args,
            &mut self.data_cache,
            gas_meter,
            &mut self.native_extensions,
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
    pub fn finish(self) -> VMResult<(ChangeSet, Vec<Event>)> {
        self.data_cache
            .into_effects()
            .map_err(|e| e.finish(Location::Undefined))
    }

    /// Same like `finish`, but also extracts the native context extensions from the session.
    pub fn finish_with_extensions(
        self,
    ) -> VMResult<(ChangeSet, Vec<Event>, NativeContextExtensions<'r>)> {
        let Session {
            data_cache,
            native_extensions,
            ..
        } = self;
        let (change_set, events) = data_cache
            .into_effects()
            .map_err(|e| e.finish(Location::Undefined))?;
        Ok((change_set, events, native_extensions))
    }

    /// Load a script and all of its types into cache
    pub fn load_script(
        &self,
        script: impl Borrow<[u8]>,
        ty_args: Vec<TypeTag>,
    ) -> VMResult<LoadedFunctionInstantiation> {
        let (_, instantiation) =
            self.runtime
                .loader()
                .load_script(script.borrow(), &ty_args, &self.data_cache)?;
        Ok(instantiation)
    }

    /// Load a module, a function, and all of its types into cache
    pub fn load_function(
        &self,
        module_id: &ModuleId,
        function_name: &IdentStr,
        type_arguments: &[TypeTag],
    ) -> VMResult<LoadedFunctionInstantiation> {
        let (_, _, _, instantiation) = self.runtime.loader().load_function(
            module_id,
            function_name,
            type_arguments,
            &self.data_cache,
        )?;
        Ok(instantiation)
    }

    pub fn load_type(&self, type_tag: &TypeTag) -> VMResult<Type> {
        self.runtime.loader().load_type(type_tag, &self.data_cache)
    }

    pub fn get_type_layout(&self, type_tag: &TypeTag) -> VMResult<MoveTypeLayout> {
        self.runtime
            .loader()
            .get_type_layout(type_tag, &self.data_cache)
    }

    pub fn get_fully_annotated_type_layout(&self, type_tag: &TypeTag) -> VMResult<MoveTypeLayout> {
        self.runtime
            .loader()
            .get_fully_annotated_type_layout(type_tag, &self.data_cache)
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
    pub type_arguments: Vec<Type>,
    pub parameters: Vec<Type>,
    pub return_: Vec<Type>,
}
