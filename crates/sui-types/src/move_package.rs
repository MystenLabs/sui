// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::execution_status::PackageUpgradeError;
use crate::{
    base_types::{ObjectID, SequenceNumber},
    crypto::DefaultHash,
    error::{ExecutionError, ExecutionErrorKind, SuiError, SuiResult},
    id::{ID, UID},
    object::OBJECT_START_VERSION,
    SUI_FRAMEWORK_ADDRESS,
};
use derive_more::Display;
use fastcrypto::hash::HashFunction;
use move_binary_format::file_format::CompiledModule;
use move_binary_format::normalized;
use move_binary_format::{
    access::ModuleAccess,
    compatibility::{Compatibility, InclusionCheck},
    errors::PartialVMResult,
};
use move_binary_format::{binary_views::BinaryIndexedView, file_format::AbilitySet};
use move_core_types::{
    account_address::AccountAddress,
    ident_str,
    identifier::{IdentStr, Identifier},
    language_storage::StructTag,
};
use move_disassembler::disassembler::Disassembler;
use move_ir_types::location::Spanned;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_with::serde_as;
use serde_with::Bytes;
use std::collections::{BTreeMap, BTreeSet};
use sui_protocol_config::ProtocolConfig;

// TODO: robust MovePackage tests
// #[cfg(test)]
// #[path = "unit_tests/move_package.rs"]
// mod base_types_tests;

pub const PACKAGE_MODULE_NAME: &IdentStr = ident_str!("package");
pub const UPGRADECAP_STRUCT_NAME: &IdentStr = ident_str!("UpgradeCap");
pub const UPGRADETICKET_STRUCT_NAME: &IdentStr = ident_str!("UpgradeTicket");
pub const UPGRADERECEIPT_STRUCT_NAME: &IdentStr = ident_str!("UpgradeReceipt");

#[derive(Clone, Debug)]
/// Additional information about a function
pub struct FnInfo {
    /// If true, it's a function involved in testing (`[test]`, `[test_only]`, `[expected_failure]`)
    pub is_test: bool,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
/// Uniquely identifies a function in a module
pub struct FnInfoKey {
    pub fn_name: String,
    pub mod_addr: AccountAddress,
}

/// A map from function info keys to function info
pub type FnInfoMap = BTreeMap<FnInfoKey, FnInfo>;

/// Identifies a struct and the module it was defined in
#[derive(
    Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Deserialize, Serialize, Hash, JsonSchema,
)]
pub struct TypeOrigin {
    pub module_name: String,
    pub struct_name: String,
    pub package: ObjectID,
}

/// Upgraded package info for the linkage table
#[derive(Eq, PartialEq, Debug, Clone, Deserialize, Serialize, Hash, JsonSchema)]
pub struct UpgradeInfo {
    /// ID of the upgraded packages
    pub upgraded_id: ObjectID,
    /// Version of the upgraded package
    pub upgraded_version: SequenceNumber,
}

// serde_bytes::ByteBuf is an analog of Vec<u8> with built-in fast serialization.
#[serde_as]
#[derive(Eq, PartialEq, Debug, Clone, Deserialize, Serialize, Hash)]
pub struct MovePackage {
    id: ObjectID,
    /// Most move packages are uniquely identified by their ID (i.e. there is only one version per
    /// ID), but the version is still stored because one package may be an upgrade of another (at a
    /// different ID), in which case its version will be one greater than the version of the
    /// upgraded package.
    ///
    /// Framework packages are an exception to this rule -- all versions of the framework packages
    /// exist at the same ID, at increasing versions.
    ///
    /// In all cases, packages are referred to by move calls using just their ID, and they are
    /// always loaded at their latest version.
    version: SequenceNumber,
    // TODO use session cache
    #[serde_as(as = "BTreeMap<_, Bytes>")]
    module_map: BTreeMap<String, Vec<u8>>,

    /// Maps struct/module to a package version where it was first defined, stored as a vector for
    /// simple serialization and deserialization.
    type_origin_table: Vec<TypeOrigin>,

    // For each dependency, maps original package ID to the info about the (upgraded) dependency
    // version that this package is using
    linkage_table: BTreeMap<ObjectID, UpgradeInfo>,
}

// NB: do _not_ add `Serialize` or `Deserialize` to this enum. Convert to u8 first  or use the
// associated constants before storing in any serialization setting.
/// Rust representation of upgrade policy constants in `sui::package`.
#[repr(u8)]
#[derive(Display, Debug, Clone, Copy)]
pub enum UpgradePolicy {
    #[display(fmt = "COMPATIBLE")]
    Compatible = 0,
    #[display(fmt = "ADDITIVE")]
    Additive = 128,
    #[display(fmt = "DEP_ONLY")]
    DepOnly = 192,
}

impl UpgradePolicy {
    /// Convenience accessors to the upgrade policies as u8s.
    pub const COMPATIBLE: u8 = Self::Compatible as u8;
    pub const ADDITIVE: u8 = Self::Additive as u8;
    pub const DEP_ONLY: u8 = Self::DepOnly as u8;

    pub fn is_valid_policy(policy: &u8) -> bool {
        Self::try_from(*policy).is_ok()
    }

    fn compatibility_check_for_protocol(protocol_config: &ProtocolConfig) -> Compatibility {
        let disallowed_new_abilities = if protocol_config.disallow_adding_abilities_on_upgrade() {
            AbilitySet::ALL
        } else {
            AbilitySet::EMPTY
        };
        Compatibility {
            check_struct_and_pub_function_linking: true,
            check_struct_layout: true,
            check_friend_linking: false,
            check_private_entry_linking: false,
            disallowed_new_abilities,
            disallow_change_struct_type_params: protocol_config
                .disallow_change_struct_type_params_on_upgrade(),
        }
    }

    pub fn check_compatibility(
        &self,
        old_module: &normalized::Module,
        new_module: &normalized::Module,
        protocol_config: &ProtocolConfig,
    ) -> PartialVMResult<()> {
        match self {
            Self::Compatible => Self::compatibility_check_for_protocol(protocol_config)
                .check(old_module, new_module),
            Self::Additive => InclusionCheck::Subset.check(old_module, new_module),
            Self::DepOnly => InclusionCheck::Equal.check(old_module, new_module),
        }
    }
}

impl TryFrom<u8> for UpgradePolicy {
    type Error = ();
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            x if x == Self::Compatible as u8 => Ok(Self::Compatible),
            x if x == Self::Additive as u8 => Ok(Self::Additive),
            x if x == Self::DepOnly as u8 => Ok(Self::DepOnly),
            _ => Err(()),
        }
    }
}

/// Rust representation of `sui::package::UpgradeCap`.
#[derive(Debug, Serialize, Deserialize)]
pub struct UpgradeCap {
    pub id: UID,
    pub package: ID,
    pub version: u64,
    pub policy: u8,
}

/// Rust representation of `sui::package::UpgradeTicket`.
#[derive(Debug, Serialize, Deserialize)]
pub struct UpgradeTicket {
    pub cap: ID,
    pub package: ID,
    pub policy: u8,
    pub digest: Vec<u8>,
}

/// Rust representation of `sui::package::UpgradeReceipt`.
#[derive(Debug, Serialize, Deserialize)]
pub struct UpgradeReceipt {
    pub cap: ID,
    pub package: ID,
}

impl MovePackage {
    /// Create a package with all required data (including serialized modules, type origin and
    /// linkage tables) already supplied.
    pub fn new(
        id: ObjectID,
        version: SequenceNumber,
        module_map: BTreeMap<String, Vec<u8>>,
        max_move_package_size: u64,
        type_origin_table: Vec<TypeOrigin>,
        linkage_table: BTreeMap<ObjectID, UpgradeInfo>,
    ) -> Result<Self, ExecutionError> {
        let pkg = Self {
            id,
            version,
            module_map,
            type_origin_table,
            linkage_table,
        };
        let object_size = pkg.size() as u64;
        if object_size > max_move_package_size {
            return Err(ExecutionErrorKind::MovePackageTooBig {
                object_size,
                max_object_size: max_move_package_size,
            }
            .into());
        }
        Ok(pkg)
    }

    pub fn digest(&self, hash_modules: bool) -> [u8; 32] {
        Self::compute_digest_for_modules_and_deps(
            self.module_map.values(),
            self.linkage_table
                .values()
                .map(|UpgradeInfo { upgraded_id, .. }| upgraded_id),
            hash_modules,
        )
    }

    /// It is important that this function is shared across both the calculation of the
    /// digest for the package, and the calculation of the digest on-chain.
    pub fn compute_digest_for_modules_and_deps<'a>(
        modules: impl IntoIterator<Item = &'a Vec<u8>>,
        object_ids: impl IntoIterator<Item = &'a ObjectID>,
        hash_modules: bool,
    ) -> [u8; 32] {
        let mut module_digests: Vec<[u8; 32]>;
        let mut components: Vec<&[u8]> = vec![];
        if !hash_modules {
            for module in modules {
                components.push(module.as_ref())
            }
        } else {
            module_digests = vec![];
            for module in modules {
                let mut digest = DefaultHash::default();
                digest.update(module);
                module_digests.push(digest.finalize().digest);
            }
            components.extend(module_digests.iter().map(|d| d.as_ref()))
        }

        components.extend(object_ids.into_iter().map(|o| o.as_ref()));
        // NB: sorting so the order of the modules and the order of the dependencies does not matter.
        components.sort();

        let mut digest = DefaultHash::default();
        for c in components {
            digest.update(c);
        }
        digest.finalize().digest
    }

    /// Create an initial version of the package along with this version's type origin and linkage
    /// tables.
    pub fn new_initial<'p>(
        modules: &[CompiledModule],
        max_move_package_size: u64,
        transitive_dependencies: impl IntoIterator<Item = &'p MovePackage>,
    ) -> Result<Self, ExecutionError> {
        let module = modules
            .first()
            .expect("Tried to build a Move package from an empty iterator of Compiled modules");
        let runtime_id = ObjectID::from(*module.address());
        let storage_id = runtime_id;
        let type_origin_table = build_initial_type_origin_table(modules);
        Self::from_module_iter_with_type_origin_table(
            storage_id,
            runtime_id,
            OBJECT_START_VERSION,
            modules,
            max_move_package_size,
            type_origin_table,
            transitive_dependencies,
        )
    }

    /// Create an upgraded version of the package along with this version's type origin and linkage
    /// tables.
    pub fn new_upgraded<'p>(
        &self,
        storage_id: ObjectID,
        modules: &[CompiledModule],
        protocol_config: &ProtocolConfig,
        transitive_dependencies: impl IntoIterator<Item = &'p MovePackage>,
    ) -> Result<Self, ExecutionError> {
        let module = modules
            .first()
            .expect("Tried to build a Move package from an empty iterator of Compiled modules");
        let runtime_id = ObjectID::from(*module.address());
        let type_origin_table =
            build_upgraded_type_origin_table(self, modules, storage_id, protocol_config)?;
        let mut new_version = self.version();
        new_version.increment();
        Self::from_module_iter_with_type_origin_table(
            storage_id,
            runtime_id,
            new_version,
            modules,
            protocol_config.max_move_package_size(),
            type_origin_table,
            transitive_dependencies,
        )
    }

    pub fn new_system(
        version: SequenceNumber,
        modules: &[CompiledModule],
        dependencies: impl IntoIterator<Item = ObjectID>,
    ) -> Self {
        let module = modules
            .first()
            .expect("Tried to build a Move package from an empty iterator of Compiled modules");

        let storage_id = ObjectID::from(*module.address());
        let type_origin_table = build_initial_type_origin_table(modules);

        let linkage_table = BTreeMap::from_iter(dependencies.into_iter().map(|dep| {
            let info = UpgradeInfo {
                upgraded_id: dep,
                // The upgraded version is used by other packages that transitively depend on this
                // system package, to make sure that if they choose a different version to depend on
                // compared to their dependencies, they pick a greater version.
                //
                // However, in the case of system packages, although they can be upgraded, unlike
                // other packages, only one version can be in use on the network at any given time,
                // so it is not possible for a package to require a different system package version
                // compared to its dependencies.
                //
                // This reason, coupled with the fact that system packages can only depend on each
                // other, mean that their own linkage tables always report a version of zero.
                upgraded_version: SequenceNumber::new(),
            };
            (dep, info)
        }));

        let module_map = BTreeMap::from_iter(modules.iter().map(|module| {
            let name = module.name().to_string();
            let mut bytes = Vec::new();
            module.serialize(&mut bytes).unwrap();
            (name, bytes)
        }));

        Self::new(
            storage_id,
            version,
            module_map,
            u64::MAX, // System packages are not subject to the size limit
            type_origin_table,
            linkage_table,
        )
        .expect("System packages are not subject to a size limit")
    }

    fn from_module_iter_with_type_origin_table<'p>(
        storage_id: ObjectID,
        self_id: ObjectID,
        version: SequenceNumber,
        modules: &[CompiledModule],
        max_move_package_size: u64,
        type_origin_table: Vec<TypeOrigin>,
        transitive_dependencies: impl IntoIterator<Item = &'p MovePackage>,
    ) -> Result<Self, ExecutionError> {
        let mut module_map = BTreeMap::new();
        let mut immediate_dependencies = BTreeSet::new();

        for module in modules {
            let name = module.name().to_string();

            immediate_dependencies.extend(
                module
                    .immediate_dependencies()
                    .into_iter()
                    .map(|dep| ObjectID::from(*dep.address())),
            );

            let mut bytes = Vec::new();
            module.serialize(&mut bytes).unwrap();
            module_map.insert(name, bytes);
        }

        immediate_dependencies.remove(&self_id);
        let linkage_table = build_linkage_table(immediate_dependencies, transitive_dependencies)?;
        Self::new(
            storage_id,
            version,
            module_map,
            max_move_package_size,
            type_origin_table,
            linkage_table,
        )
    }

    /// Return the size of the package in bytes
    pub fn size(&self) -> usize {
        let module_map_size = self
            .module_map
            .iter()
            .map(|(name, module)| name.len() + module.len())
            .sum::<usize>();
        let type_origin_table_size = self
            .type_origin_table
            .iter()
            .map(
                |TypeOrigin {
                     module_name,
                     struct_name,
                     ..
                 }| module_name.len() + struct_name.len() + ObjectID::LENGTH,
            )
            .sum::<usize>();

        let linkage_table_size = self.linkage_table.len()
            * (ObjectID::LENGTH + (ObjectID::LENGTH + 8/* SequenceNumber */));

        8 /* SequenceNumber */ + module_map_size + type_origin_table_size + linkage_table_size
    }

    pub fn id(&self) -> ObjectID {
        self.id
    }

    pub fn version(&self) -> SequenceNumber {
        self.version
    }

    pub fn decrement_version(&mut self) {
        self.version.decrement();
    }

    pub fn increment_version(&mut self) {
        self.version.increment();
    }

    /// Approximate size of the package in bytes. This is used for gas metering.
    pub fn object_size_for_gas_metering(&self) -> usize {
        self.size()
    }

    pub fn serialized_module_map(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.module_map
    }

    pub fn type_origin_table(&self) -> &Vec<TypeOrigin> {
        &self.type_origin_table
    }

    pub fn type_origin_map(&self) -> BTreeMap<(String, String), ObjectID> {
        self.type_origin_table
            .iter()
            .map(
                |TypeOrigin {
                     module_name,
                     struct_name,
                     package,
                 }| { ((module_name.clone(), struct_name.clone()), *package) },
            )
            .collect()
    }

    pub fn linkage_table(&self) -> &BTreeMap<ObjectID, UpgradeInfo> {
        &self.linkage_table
    }

    /// The ObjectID that this package's modules believe they are from, at runtime (can differ from
    /// `MovePackage::id()` in the case of package upgrades).
    pub fn original_package_id(&self) -> ObjectID {
        let bytes = self.module_map.values().next().expect("Empty module map");
        let module = CompiledModule::deserialize_with_defaults(bytes)
            .expect("A Move package contains a module that cannot be deserialized");
        (*module.address()).into()
    }

    pub fn deserialize_module(
        &self,
        module: &Identifier,
        max_binary_format_version: u32,
        check_no_bytes_remaining: bool,
    ) -> SuiResult<CompiledModule> {
        // TODO use the session's cache
        let bytes = self
            .serialized_module_map()
            .get(module.as_str())
            .ok_or_else(|| SuiError::ModuleNotFound {
                module_name: module.to_string(),
            })?;
        CompiledModule::deserialize_with_config(
            bytes,
            max_binary_format_version,
            check_no_bytes_remaining,
        )
        .map_err(|error| SuiError::ModuleDeserializationFailure {
            error: error.to_string(),
        })
    }

    pub fn disassemble(&self) -> SuiResult<BTreeMap<String, Value>> {
        disassemble_modules(self.module_map.values())
    }

    pub fn normalize(
        &self,
        max_binary_format_version: u32,
        check_no_bytes_remaining: bool,
    ) -> SuiResult<BTreeMap<String, normalized::Module>> {
        normalize_modules(
            self.module_map.values(),
            max_binary_format_version,
            check_no_bytes_remaining,
        )
    }
}

impl UpgradeCap {
    pub fn type_() -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: PACKAGE_MODULE_NAME.to_owned(),
            name: UPGRADECAP_STRUCT_NAME.to_owned(),
            type_params: vec![],
        }
    }

    /// Create an `UpgradeCap` for the newly published package at `package_id`, and associate it with
    /// the fresh `uid`.
    pub fn new(uid: ObjectID, package_id: ObjectID) -> Self {
        UpgradeCap {
            id: UID::new(uid),
            package: ID::new(package_id),
            version: 1,
            policy: UpgradePolicy::COMPATIBLE,
        }
    }
}

impl UpgradeTicket {
    pub fn type_() -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: PACKAGE_MODULE_NAME.to_owned(),
            name: UPGRADETICKET_STRUCT_NAME.to_owned(),
            type_params: vec![],
        }
    }
}

impl UpgradeReceipt {
    pub fn type_() -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: PACKAGE_MODULE_NAME.to_owned(),
            name: UPGRADERECEIPT_STRUCT_NAME.to_owned(),
            type_params: vec![],
        }
    }

    /// Create an `UpgradeReceipt` for the upgraded package at `package_id` using the
    /// `UpgradeTicket` and newly published package id.
    pub fn new(upgrade_ticket: UpgradeTicket, upgraded_package_id: ObjectID) -> Self {
        UpgradeReceipt {
            cap: upgrade_ticket.cap,
            package: ID::new(upgraded_package_id),
        }
    }
}

/// Checks if a function is annotated with one of the test-related annotations
pub fn is_test_fun(name: &IdentStr, module: &CompiledModule, fn_info_map: &FnInfoMap) -> bool {
    let fn_name = name.to_string();
    let mod_handle = module.self_handle();
    let mod_addr = *module.address_identifier_at(mod_handle.address);
    let fn_info_key = FnInfoKey { fn_name, mod_addr };
    match fn_info_map.get(&fn_info_key) {
        Some(fn_info) => fn_info.is_test,
        None => false,
    }
}

pub fn disassemble_modules<'a, I>(modules: I) -> SuiResult<BTreeMap<String, Value>>
where
    I: Iterator<Item = &'a Vec<u8>>,
{
    let mut disassembled = BTreeMap::new();
    for bytecode in modules {
        // this function is only from JSON RPC - it is OK to deserialize with max Move binary
        // version
        let module = CompiledModule::deserialize_with_defaults(bytecode).map_err(|error| {
            SuiError::ModuleDeserializationFailure {
                error: error.to_string(),
            }
        })?;
        let view = BinaryIndexedView::Module(&module);
        let d = Disassembler::from_view(view, Spanned::unsafe_no_loc(()).loc).map_err(|e| {
            SuiError::ObjectSerializationError {
                error: e.to_string(),
            }
        })?;
        let bytecode_str = d
            .disassemble()
            .map_err(|e| SuiError::ObjectSerializationError {
                error: e.to_string(),
            })?;
        disassembled.insert(module.name().to_string(), Value::String(bytecode_str));
    }
    Ok(disassembled)
}

pub fn normalize_modules<'a, I>(
    modules: I,
    max_binary_format_version: u32,
    check_no_bytes_remaining: bool,
) -> SuiResult<BTreeMap<String, normalized::Module>>
where
    I: Iterator<Item = &'a Vec<u8>>,
{
    let mut normalized_modules = BTreeMap::new();
    for bytecode in modules {
        let module = CompiledModule::deserialize_with_config(
            bytecode,
            max_binary_format_version,
            check_no_bytes_remaining,
        )
        .map_err(|error| SuiError::ModuleDeserializationFailure {
            error: error.to_string(),
        })?;
        let normalized_module = normalized::Module::new(&module);
        normalized_modules.insert(normalized_module.name.to_string(), normalized_module);
    }
    Ok(normalized_modules)
}

pub fn normalize_deserialized_modules<'a, I>(modules: I) -> BTreeMap<String, normalized::Module>
where
    I: Iterator<Item = &'a CompiledModule>,
{
    let mut normalized_modules = BTreeMap::new();
    for module in modules {
        let normalized_module = normalized::Module::new(module);
        normalized_modules.insert(normalized_module.name.to_string(), normalized_module);
    }
    normalized_modules
}

fn build_linkage_table<'p>(
    mut immediate_dependencies: BTreeSet<ObjectID>,
    transitive_dependencies: impl IntoIterator<Item = &'p MovePackage>,
) -> Result<BTreeMap<ObjectID, UpgradeInfo>, ExecutionError> {
    let mut linkage_table = BTreeMap::new();
    let mut dep_linkage_tables = vec![];

    for transitive_dep in transitive_dependencies.into_iter() {
        // original_package_id will deserialize a module but only for the purpose of obtaining
        // "original ID" of the package containing it so using max Move binary version during
        // deserialization is OK
        let original_id = transitive_dep.original_package_id();

        if immediate_dependencies.remove(&original_id) {
            // Found an immediate dependency, mark it as seen, and stash a reference to its linkage
            // table to check later.
            dep_linkage_tables.push(&transitive_dep.linkage_table);
        }

        linkage_table.insert(
            original_id,
            UpgradeInfo {
                upgraded_id: transitive_dep.id,
                upgraded_version: transitive_dep.version,
            },
        );
    }
    // (1) Every dependency is represented in the transitive dependencies
    if !immediate_dependencies.is_empty() {
        return Err(ExecutionErrorKind::PublishUpgradeMissingDependency.into());
    }

    // (2) Every dependency's linkage table is superseded by this linkage table
    for dep_linkage_table in dep_linkage_tables {
        for (original_id, dep_info) in dep_linkage_table {
            let Some(our_info) = linkage_table.get(original_id) else {
                return Err(ExecutionErrorKind::PublishUpgradeMissingDependency.into());
            };

            if our_info.upgraded_version < dep_info.upgraded_version {
                return Err(ExecutionErrorKind::PublishUpgradeDependencyDowngrade.into());
            }
        }
    }

    Ok(linkage_table)
}

fn build_initial_type_origin_table(modules: &[CompiledModule]) -> Vec<TypeOrigin> {
    modules
        .iter()
        .flat_map(|m| {
            m.struct_defs().iter().map(|struct_def| {
                let struct_handle = m.struct_handle_at(struct_def.struct_handle);
                let module_name = m.name().to_string();
                let struct_name = m.identifier_at(struct_handle.name).to_string();
                let package: ObjectID = (*m.self_id().address()).into();
                TypeOrigin {
                    module_name,
                    struct_name,
                    package,
                }
            })
        })
        .collect()
}

fn build_upgraded_type_origin_table(
    predecessor: &MovePackage,
    modules: &[CompiledModule],
    storage_id: ObjectID,
    protocol_config: &ProtocolConfig,
) -> Result<Vec<TypeOrigin>, ExecutionError> {
    let mut new_table = vec![];
    let mut existing_table = predecessor.type_origin_map();
    for m in modules {
        for struct_def in m.struct_defs() {
            let struct_handle = m.struct_handle_at(struct_def.struct_handle);
            let module_name = m.name().to_string();
            let struct_name = m.identifier_at(struct_handle.name).to_string();
            let mod_key = (module_name.clone(), struct_name.clone());
            // if id exists in the predecessor's table, use it, otherwise use the id of the upgraded
            // module
            let package = existing_table.remove(&mod_key).unwrap_or(storage_id);
            new_table.push(TypeOrigin {
                module_name,
                struct_name,
                package,
            });
        }
    }

    if !existing_table.is_empty() {
        if protocol_config.missing_type_is_compatibility_error() {
            Err(ExecutionError::from_kind(
                ExecutionErrorKind::PackageUpgradeError {
                    upgrade_error: PackageUpgradeError::IncompatibleUpgrade,
                },
            ))
        } else {
            Err(ExecutionError::invariant_violation(
                "Package upgrade missing type from previous version.",
            ))
        }
    } else {
        Ok(new_table)
    }
}
