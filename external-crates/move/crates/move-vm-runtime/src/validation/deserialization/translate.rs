use crate::validation::deserialization::ast::Package;

use move_binary_format::{
    errors::{Location, PartialVMError, VMResult},
    CompiledModule,
};
use move_core_types::{resolver::SerializedPackage, vm_status::StatusCode};
use move_vm_config::runtime::VMConfig;

use std::collections::BTreeMap;

pub(crate) fn package(vm_config: &VMConfig, pkg: SerializedPackage) -> VMResult<Package> {
    let mut modules = BTreeMap::new();
    for (mname, module) in pkg.modules.iter() {
        let module = CompiledModule::deserialize_with_config(module, &vm_config.binary_config)
            // TODO(vm-rewrite): add Location::Package
            .map_err(|err| err.finish(Location::Undefined))?;
        // The name of the module in the mapping, and the name of the module itself should be equal
        assert_eq!(mname.as_ident_str(), module.self_id().name());
        modules.insert(module.self_id(), module);
    }

    // Packages must be non-empty
    if modules.is_empty() {
        return Err(PartialVMError::new(StatusCode::EMPTY_PACKAGE)
            .with_message("Empty packages are not allowed.".to_string())
            .finish(Location::Undefined));
    }

    let runtime_id = *modules.keys().next().expect("non-empty package").address();

    Ok(Package::new(
        runtime_id,
        modules.into_values().collect(),
        pkg,
    ))
}
