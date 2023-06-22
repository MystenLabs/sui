// use sui_adapter::adapter::MoveVM;
// use sui_adapter::{execution_engine, execution_mode};
// use sui_types::messages::{InputObjectKind, InputObjects, TransactionDataAPI, VerifiedTransaction};
use sui_types::storage::get_module_by_id;

use anyhow::Result;
use move_binary_format::CompiledModule;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::{language_storage::ModuleId, resolver::ModuleResolver};
use std::collections::HashMap;
use sui_types::{
    base_types::{ObjectID, ObjectRef, VersionNumber},
    error::{SuiError, SuiResult},
    object::Object,
    storage::{BackingPackageStore, ChildObjectResolver, ObjectStore, ParentSync},
};
