// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::storage::{BackingPackageStore, ChildObjectResolver, StorageView};

pub trait SuiResolver: BackingPackageStore {
    fn as_backing_package_store(&self) -> &dyn BackingPackageStore;
}

impl<T> SuiResolver for T
where
    T: BackingPackageStore,
{
    fn as_backing_package_store(&self) -> &dyn BackingPackageStore {
        self
    }
}

/// Interface with the store necessary to execute a programmable transaction
pub trait ExecutionState: StorageView + SuiResolver {
    fn as_child_resolver(&self) -> &dyn ChildObjectResolver;
}

impl<T> ExecutionState for T
where
    T: StorageView,
    T: SuiResolver,
{
    fn as_child_resolver(&self) -> &dyn ChildObjectResolver {
        self
    }
}
