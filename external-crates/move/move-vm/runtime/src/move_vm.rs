// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use crate::{
    config::VMConfig, data_cache::TransactionDataCache, native_extensions::NativeContextExtensions,
    native_functions::NativeFunction, runtime::VMRuntime, session::Session,
};
use move_binary_format::{
    errors::{Location, VMResult},
    CompiledModule,
};
use move_core_types::{
    account_address::AccountAddress, identifier::Identifier, language_storage::ModuleId,
    metadata::Metadata, resolver::MoveResolver,
};

pub struct MoveVM {
    runtime: VMRuntime,
}

impl MoveVM {
    pub fn new(
        natives: impl IntoIterator<Item = (AccountAddress, Identifier, Identifier, NativeFunction)>,
    ) -> VMResult<Self> {
        Self::new_with_config(natives, VMConfig::default())
    }

    pub fn new_with_config(
        natives: impl IntoIterator<Item = (AccountAddress, Identifier, Identifier, NativeFunction)>,
        vm_config: VMConfig,
    ) -> VMResult<Self> {
        Ok(Self {
            runtime: VMRuntime::new(natives, vm_config)
                .map_err(|err| err.finish(Location::Undefined))?,
        })
    }

    /// Create a new Session backed by the given storage.
    ///
    /// Right now it is the caller's responsibility to ensure cache coherence of the Move VM Loader
    ///   - When a module gets published in a Move VM Session, and then gets used by another
    ///     transaction, it will be loaded into the code cache and stay there even if the resulted
    ///     effects do not get commited back to the storage when the Session ends.
    ///   - As a result, if one wants to have multiple sessions at a time, one needs to make sure
    ///     none of them will try to publish a module. In other words, if there is a module publishing
    ///     Session it must be the only Session existing.
    ///   - In general, a new Move VM needs to be created whenever the storage gets modified by an
    ///     outer envrionment, or otherwise the states may be out of sync. There are a few exceptional
    ///     cases where this may not be necessary, with the most notable one being the common module
    ///     publishing flow: you can keep using the same Move VM if you publish some modules in a Session
    ///     and apply the effects to the storage when the Session ends.
    pub fn new_session<'r, S: MoveResolver>(&self, remote: S) -> Session<'r, '_, S> {
        self.runtime.new_session(remote)
    }

    /// Create a new session, as in `new_session`, but provide native context extensions.
    pub fn new_session_with_extensions<'r, S: MoveResolver>(
        &self,
        remote: S,
        extensions: NativeContextExtensions<'r>,
    ) -> Session<'r, '_, S> {
        self.runtime.new_session_with_extensions(remote, extensions)
    }

    /// Load a module into VM's code cache
    pub fn load_module<S: MoveResolver>(
        &self,
        module_id: &ModuleId,
        remote: S,
    ) -> VMResult<Arc<CompiledModule>> {
        self.runtime
            .loader()
            .load_module(
                module_id,
                &TransactionDataCache::new(remote, self.runtime.loader()),
            )
            .map(|(compiled, _)| compiled)
    }

    /// Attempts to discover metadata in a given module with given key. Availability
    /// of this data may depend on multiple aspects. In general, no hard assumptions of
    /// availability should be made, but typically, one can expect that
    /// the modules which have been involved in the execution of the last session are available.
    ///
    /// This is called by an adapter to extract, for example, debug information out of
    /// the metadata section of the code for post mortem analysis. Notice that because
    /// of ownership of the underlying binary representation of modules hidden behind an rwlock,
    /// this actually has to hand back a copy of the associated metadata, so metadata should
    /// be organized keeping this in mind.
    ///
    /// TODO: in the new loader architecture, as the loader is visible to the adapter, one would
    ///   call this directly via the loader instead of the VM.
    pub fn get_module_metadata(&self, module: ModuleId, key: &[u8]) -> Option<Metadata> {
        self.runtime.loader().get_metadata(module, key)
    }
}
