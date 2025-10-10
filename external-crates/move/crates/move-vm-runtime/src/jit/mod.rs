// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod execution;
pub mod optimization;

use crate::{
    cache::identifier_interner::IdentifierInterner,
    jit::{
        execution::ast::Package,
        optimization::{optimize, to_optimized_form},
    },
    natives::functions::NativeFunctions,
    validation::verification,
};
use move_binary_format::errors::PartialVMResult;
use move_vm_config::runtime::VMConfig;

pub fn translate_package(
    vm_config: &VMConfig,
    interner: &IdentifierInterner,
    natives: &NativeFunctions,
    loaded_package: verification::ast::Package,
) -> PartialVMResult<Package> {
    let opt_package = if vm_config.optimize_bytecode {
        optimize(loaded_package)
    } else {
        to_optimized_form(loaded_package)
    };
    execution::translate::package(natives, interner, opt_package)
}

// TODO: move this to unit testing folder.
#[cfg(test)]
mod tests {
    use super::translate_package;
    use crate::{
        cache::identifier_interner::IdentifierInterner,
        jit::execution::ast::Package as RuntimePackage, validation::verification::ast as verif_ast,
    };
    use indexmap::IndexMap;
    use move_binary_format::file_format::empty_module;
    use move_core_types::{account_address::AccountAddress, language_storage::ModuleId};
    use move_vm_config::runtime::VMConfig;
    use std::collections::BTreeMap;

    fn make_verified_empty_package(
        original_id: AccountAddress,
        version_id: AccountAddress,
    ) -> verif_ast::Package {
        // Minimal valid module
        let module = empty_module();
        let module_id: ModuleId = module.self_id();

        // Assemble verification package with a single module and minimal tables
        verif_ast::Package {
            original_id,
            version_id,
            modules: BTreeMap::from([(module_id, verif_ast::Module { value: module })]),
            type_origin_table: IndexMap::new(),
            linkage_table: BTreeMap::from([(original_id, version_id)]),
            version: 0,
        }
    }

    fn assert_basic_runtime_pkg(
        pkg: &RuntimePackage,
        original_id: AccountAddress,
        version_id: AccountAddress,
    ) {
        assert_eq!(pkg.original_id, original_id);
        assert_eq!(pkg.version_id, version_id);
        // One module translated from the single compiled module
        assert_eq!(pkg.loaded_modules.len(), 1);
    }

    #[test]
    fn translate_without_optimization() {
        let original_id = AccountAddress::from([1u8; 32]);
        let version_id = AccountAddress::from([2u8; 32]);
        let verified = make_verified_empty_package(original_id, version_id);

        let vm_config = VMConfig {
            optimize_bytecode: false,
            ..VMConfig::default()
        };
        let natives = crate::natives::functions::NativeFunctions::empty_for_testing().unwrap();
        let interner = IdentifierInterner::new();

        let result = translate_package(&vm_config, &interner, &natives, verified);
        let runtime_pkg = result.expect("translate_package should succeed for minimal package");
        assert_basic_runtime_pkg(&runtime_pkg, original_id, version_id);
    }

    #[test]
    fn translate_with_optimization() {
        let original_id = AccountAddress::from([3u8; 32]);
        let version_id = AccountAddress::from([4u8; 32]);
        let verified = make_verified_empty_package(original_id, version_id);

        let vm_config = VMConfig {
            optimize_bytecode: true,
            ..VMConfig::default()
        };
        let natives = crate::natives::functions::NativeFunctions::empty_for_testing().unwrap();
        let interner = IdentifierInterner::new();

        let result = translate_package(&vm_config, &interner, &natives, verified);
        let runtime_pkg = result.expect("translate_package should succeed for minimal package");
        assert_basic_runtime_pkg(&runtime_pkg, original_id, version_id);
    }
}
