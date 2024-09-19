pub mod runtime;

use crate::{
    cache::type_cache::TypeCache,
    jit::runtime::ast::Package,
    natives::functions::NativeFunctions,
    on_chain::ast::{DeserializedPackage, PackageStorageId},
};

use move_binary_format::errors::PartialVMResult;
use move_vm_types::data_store::DataStore;

use parking_lot::RwLock;

pub fn translate_package(
    natives: &NativeFunctions,
    type_cache: &RwLock<TypeCache>,
    data_store: &impl DataStore,
    package_key: PackageStorageId,
    loaded_package: DeserializedPackage,
) -> PartialVMResult<Package> {
    let runtime_id = loaded_package.runtime_id;
    let modules = loaded_package.into_modules();
    runtime::translate::package(
        package_key,
        runtime_id,
        modules,
        natives,
        type_cache,
        data_store,
    )
}
