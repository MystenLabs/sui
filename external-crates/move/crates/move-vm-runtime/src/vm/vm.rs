// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cache::vm_cache::VMCache, natives::extensions::NativeContextExtensions,
    natives::functions::NativeFunctions, on_chain::data_cache::TransactionDataCache,
    vm::vm_instance::VirtualMachineInstance,
};

use move_binary_format::errors::VMResult;
use move_core_types::resolver::MoveResolver;
use move_vm_config::runtime::VMConfig;

use std::sync::Arc;

pub struct VirtualMachine {
    /// The VM package cache for the VM, holding currently-loaded packages.
    cache: Arc<VMCache>,
    /// The native functions the Move VM uses
    natives: Arc<NativeFunctions>,
    /// The Move VM's configuration.
    vm_config: Arc<VMConfig>,
}

impl VirtualMachine {
    pub fn new(natives: NativeFunctions, vm_config: VMConfig) -> Self {
        let natives = Arc::new(natives);
        let vm_config = Arc::new(vm_config);
        let cache = Arc::new(VMCache::new(natives.clone(), vm_config.clone()));
        Self {
            cache,
            natives,
            vm_config,
        }
    }

    // Blocks on the loader.
    // TODO: Have this hand back a tokio Notify
    pub fn make_instance<'native, DataCache: MoveResolver>(
        &mut self,
        remote: DataCache,
    ) -> VMResult<VirtualMachineInstance<'native, DataCache>> {
        let data_cache = TransactionDataCache::new(remote);
        let virtual_tables = self.cache.generate_runtime_vtables(&data_cache)?;
        // Called and checked linkage, etc.
        let instance = VirtualMachineInstance {
            virtual_tables,
            vm_cache: self.cache.clone(),
            vm_config: self.vm_config.clone(),
            data_cache,
            native_extensions: NativeContextExtensions::default(),
        };
        Ok(instance)
    }
}
