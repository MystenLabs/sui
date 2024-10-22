// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::errors::VMResult;
use move_core_types::{
    account_address::AccountAddress, language_storage::ModuleId, resolver::SerializedPackage,
};

/// Provide an implementation for bytecodes related to data with a given data store.
///
/// The `DataStore` is a generic concept that includes both data and events.
/// A default implementation of the `DataStore` is `TransactionDataCache` which provides
/// an in memory cache for a given transaction and the atomic transactional changes
/// proper of a script execution (transaction).
pub trait DataStore {
    /// Given a list of storage IDs for a package, return the `SerializedPackage` for each ID.
    /// A result is returned for every ID requested. If any package is not found, an error is
    /// returned.
    fn load_packages_static<const N: usize>(
        &self,
        ids: [AccountAddress; N],
    ) -> VMResult<[SerializedPackage; N]>;

    fn load_packages(&self, ids: &[AccountAddress]) -> VMResult<Vec<SerializedPackage>>;

    /// Publish a module.
    fn publish_module(&mut self, module_id: &ModuleId, blob: Vec<u8>) -> VMResult<()>;
}
