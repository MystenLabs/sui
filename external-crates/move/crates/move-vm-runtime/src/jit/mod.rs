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
        optimize(loaded_package)?
    } else {
        to_optimized_form(loaded_package)?
    };
    execution::translate::package(natives, interner, opt_package)
}

#[cfg(test)]
mod tests {
    use super::translate_package;
    use crate::{
        cache::identifier_interner::IdentifierInterner,
        jit::execution::ast::Package as RuntimePackage, validation::verification::ast as verif_ast,
    };
    use indexmap::IndexMap;
    use move_binary_format::file_format::{
        Bytecode, CodeUnit, FunctionDefinition, FunctionHandle, FunctionHandleIndex,
        IdentifierIndex, ModuleHandleIndex, Signature, SignatureIndex, SignatureToken, Visibility,
        empty_module,
    };
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

    #[test]
    fn translate_module_with_return_constant() {
        let original_id = AccountAddress::from([7u8; 32]);
        let version_id = AccountAddress::from([8u8; 32]);

        // Create a module with a function that returns u64(10)
        let mut module = empty_module();

        // Add return type signature (u64)
        module.signatures.push(Signature(vec![SignatureToken::U64]));
        let return_sig_idx = SignatureIndex((module.signatures.len() - 1) as u16);

        // Add empty parameters signature
        module.signatures.push(Signature(vec![]));
        let params_sig_idx = SignatureIndex((module.signatures.len() - 1) as u16);

        // Add function handle
        let function_handle = FunctionHandle {
            module: ModuleHandleIndex(0),
            name: IdentifierIndex(0),
            parameters: params_sig_idx,
            return_: return_sig_idx,
            type_parameters: vec![],
        };
        module.function_handles.push(function_handle);

        // Create function definition with bytecode
        let function_def = FunctionDefinition {
            code: Some(CodeUnit {
                locals: params_sig_idx, // No locals needed
                code: vec![
                    Bytecode::LdU64(10), // Load constant 10
                    Bytecode::Ret,       // Return
                ],
                jump_tables: vec![],
            }),
            function: FunctionHandleIndex(0),
            visibility: Visibility::Public,
            is_entry: false,
            acquires_global_resources: vec![],
        };
        module.function_defs.push(function_def);

        // Create package with the module
        let module_id = module.self_id();
        let verified = verif_ast::Package {
            original_id,
            version_id,
            version: 0,
            modules: BTreeMap::from([(module_id, verif_ast::Module { value: module })]),
            type_origin_table: IndexMap::new(),
            linkage_table: BTreeMap::from([(original_id, version_id)]),
        };
        // First translate without optimization
        let vm_config_no_opt = VMConfig {
            optimize_bytecode: false,
            ..VMConfig::default()
        };
        let natives = crate::natives::functions::NativeFunctions::empty_for_testing().unwrap();
        let interner = IdentifierInterner::new();

        let result_no_opt =
            translate_package(&vm_config_no_opt, &interner, &natives, verified.clone());
        let runtime_pkg_no_opt =
            result_no_opt.expect("translate_package should succeed without optimization");

        // Then translate with optimization
        let vm_config_opt = VMConfig {
            optimize_bytecode: true,
            ..VMConfig::default()
        };
        let result_opt = translate_package(&vm_config_opt, &interner, &natives, verified);
        let runtime_pkg_opt =
            result_opt.expect("translate_package should succeed with optimization");

        // Verify basic properties for both packages
        assert_basic_runtime_pkg(&runtime_pkg_no_opt, original_id, version_id);
        assert_basic_runtime_pkg(&runtime_pkg_opt, original_id, version_id);

        // Verify both translations produced the same results
        assert_eq!(
            runtime_pkg_no_opt.loaded_modules.len(),
            runtime_pkg_opt.loaded_modules.len()
        );
        assert_eq!(
            runtime_pkg_no_opt.loaded_modules[0].functions.len(),
            runtime_pkg_opt.loaded_modules[0].functions.len()
        );

        // Compare the function definitions
        let func_no_opt = &runtime_pkg_no_opt.loaded_modules[0].functions[0];
        let func_opt = &runtime_pkg_opt.loaded_modules[0].functions[0];
        assert_eq!(func_no_opt.code.len(), func_opt.code.len());
    }
}
