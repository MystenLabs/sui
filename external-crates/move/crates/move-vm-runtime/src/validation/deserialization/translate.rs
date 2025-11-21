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
        assert_eq!(mname.as_ident_str(), module.self_id().name());

        assert_eq!(
            module.address(),
            &original_id,
            "Module address does not match package original ID"
        );

        // Impossible for a package to have two modules with the same name at this point.
        assert!(modules.insert(module.self_id(), module).is_none());
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
