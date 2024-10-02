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
    for module in pkg.modules.iter() {
        let module = CompiledModule::deserialize_with_config(module, &vm_config.binary_config)
            .map_err(|err| -> move_binary_format::errors::VMError {
                let msg = format!("Deserialization error: {:?}", err);
                PartialVMError::new(StatusCode::CODE_DESERIALIZATION_ERROR)
                    .with_message(msg)
                    .finish(Location::Undefined) // TODO(tzakian): add Location::Package
            })?;
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
