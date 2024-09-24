// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::errors::VMResult;
use move_core_types::{
    account_address::AccountAddress, language_storage::ModuleId,
};

/// Provide an implementation for bytecodes related to data with a given data store.
///
/// The `DataStore` is a generic concept that includes both data and events.
/// A default implementation of the `DataStore` is `TransactionDataCache` which provides
/// an in memory cache for a given transaction and the atomic transactional changes
/// proper of a script execution (transaction).
pub trait DataStore {
    /// Get the serialized format of a `CompiledModule` given a `ModuleId`.
    fn load_module(&self, module_id: &ModuleId) -> VMResult<Vec<u8>>;

    /// Get the serialized format of a package's modules given a package id.
    fn load_package(&self, package_id: &AccountAddress) -> VMResult<Vec<Vec<u8>>>;

    /// Publish a module.
    fn publish_module(&mut self, module_id: &ModuleId, blob: Vec<u8>) -> VMResult<()>;
}
