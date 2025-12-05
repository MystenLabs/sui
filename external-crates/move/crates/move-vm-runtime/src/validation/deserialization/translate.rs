use crate::validation::deserialization::ast::Package;

use move_binary_format::{
    CompiledModule, IndexKind,
    errors::{Location, PartialVMError, VMResult, verification_error},
};
use move_core_types::{resolver::SerializedPackage, vm_status::StatusCode};
use move_vm_config::runtime::VMConfig;

use std::collections::BTreeMap;

/// Deserialize a serialized package into a `Package` structure, performing basic validation.
/// 1. The module name in the mapping matches the module's self name.
/// 2. Every module's address matches the package's original ID.
/// 3. No duplicate module names exist.
/// 4. The package is non-empty (has at least one module).
pub(crate) fn package(vm_config: &VMConfig, pkg: SerializedPackage) -> VMResult<Package> {
    let original_id = pkg.original_id;

    let mut modules = BTreeMap::new();
    for (mname, module) in pkg.modules.iter() {
        let module = CompiledModule::deserialize_with_config(module, &vm_config.binary_config)
            .map_err(|err| err.finish(Location::Package(pkg.version_id)))?;

        // The name of the module in the mapping, and the name of the module itself should be equal
        if mname.as_ident_str() != module.self_id().name() {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message(format!(
                        "Module name mismatch: mapping has '{}', module has '{}'",
                        mname.as_ident_str(),
                        module.self_id().name()
                    ))
                    .finish(Location::Package(pkg.version_id)),
            );
        }

        if module.address() != &pkg.original_id {
            return Err(verification_error(
                StatusCode::MISMATCHED_MODULE_IDS_IN_PACKAGE,
                IndexKind::AddressIdentifier,
                module.self_handle_idx().0,
            )
            .finish(Location::Package(pkg.version_id)));
        }

        // Impossible for a package to have two modules with the same name at this point.
        if modules.insert(module.self_id(), module).is_some() {
            return Err(PartialVMError::new(StatusCode::DUPLICATE_MODULE_NAME)
                .with_message(format!(
                    "Duplicate module name found: '{}'",
                    mname.as_ident_str()
                ))
                .finish(Location::Package(pkg.version_id)));
        }
    }

    // Packages must be non-empty
    if modules.is_empty() {
        return Err(PartialVMError::new(StatusCode::EMPTY_PACKAGE)
            .with_message("Empty packages are not allowed.".to_string())
            .finish(Location::Package(pkg.version_id)));
    }

    Ok(Package::new(
        original_id,
        modules.into_values().collect(),
        pkg,
    ))
}
