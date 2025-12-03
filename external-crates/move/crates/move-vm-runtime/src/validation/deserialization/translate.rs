use crate::validation::deserialization::ast::Package;

use move_binary_format::{
    CompiledModule,
    errors::{Location, PartialVMError, VMResult},
};
use move_core_types::{resolver::SerializedPackage, vm_status::StatusCode};
use move_vm_config::runtime::VMConfig;

use std::collections::BTreeMap;

pub(crate) fn package(vm_config: &VMConfig, pkg: SerializedPackage) -> VMResult<Package> {
    let original_id = pkg.original_id;

    let mut modules = BTreeMap::new();
    for (mname, module) in pkg.modules.iter() {
        let module = CompiledModule::deserialize_with_config(module, &vm_config.binary_config)
            .map_err(|err| err.finish(Location::Package(pkg.version_id)))?;

        // The name of the module in the mapping, and the name of the module itself should be equal
        if mname.as_ident_str() != module.self_id().name() {
            return Err(PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                .with_message(format!(
                    "Module name mismatch: mapping has '{}', module has '{}'",
                    mname.as_ident_str(),
                    module.self_id().name()
                ))
                .finish(Location::Package(pkg.version_id)));
        }

        // The address of the module must match the original package ID
        if module.address() != &original_id {
            return Err(PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                .with_message(format!(
                    "Module address mismatch: expected '{}', found '{}'",
                    original_id,
                    module.address()
                ))
                .finish(Location::Package(pkg.version_id)));
        }

        // Impossible for a package to have two modules with the same name at this point.
        if !modules.insert(module.self_id(), module).is_none() {
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
