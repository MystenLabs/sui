// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::source_package::parsed_manifest::CustomDepInfo;
use anyhow::bail;
use move_symbol_pool::Symbol;
use once_cell::sync::Lazy;
use std::sync::Mutex;

// TODO: remove static hooks and refactor this crate for better customizability

/// A trait providing hooks to customize the package system for a particular Move application.
/// An instance of the trait can be registered globally.
pub trait PackageHooks {
    /// Returns custom fields allowed in `PackageInfo`.
    fn custom_package_info_fields(&self) -> Vec<String>;

    /// Returns a custom key for dependencies, if available. This is the string used
    /// in dependencies `{ <key> = value, address = addr }.
    fn custom_dependency_key(&self) -> Option<String>;

    /// A resolver for custom dependencies in the manifest. This is called to download the
    /// dependency from the dependency into the `info.local_path` location, similar as with git
    /// dependencies.
    fn resolve_custom_dependency(
        &self,
        dep_name: Symbol,
        info: &CustomDepInfo,
    ) -> anyhow::Result<()>;
}
static HOOKS: Lazy<Mutex<Option<Box<dyn PackageHooks + Send + Sync>>>> =
    Lazy::new(|| Mutex::new(None));

/// Registers package hooks for the process in which the package system is used.
pub fn register_package_hooks(hooks: Box<dyn PackageHooks + Send + Sync>) {
    *HOOKS.lock().unwrap() = Some(hooks)
}

/// Calls any registered hook to resolve a node dependency. Bails if none is registered.
pub(crate) fn resolve_custom_dependency(
    dep_name: Symbol,
    info: &CustomDepInfo,
) -> anyhow::Result<()> {
    if let Some(hooks) = &*HOOKS.lock().unwrap() {
        hooks.resolve_custom_dependency(dep_name, info)
    } else {
        bail!("use of unsupported custom dependency in package manifest")
    }
}

pub(crate) fn custom_dependency_key() -> Option<String> {
    if let Some(hooks) = &*HOOKS.lock().unwrap() {
        hooks.custom_dependency_key()
    } else {
        None
    }
}

/// Calls any registered hook to return custom package fields.
pub(crate) fn custom_package_info_fields() -> Vec<String> {
    if let Some(hooks) = &*HOOKS.lock().unwrap() {
        hooks.custom_package_info_fields()
    } else {
        vec![]
    }
}
