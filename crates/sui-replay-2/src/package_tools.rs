// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::Node;
use sui_data_store::{
    ObjectKey, ObjectStore, VersionQuery,
    stores::{FileSystemStore, NODE_MAPPING_FILE, OBJECTS_DIR},
};

use anyhow::{Context, Result, anyhow, bail};
use move_binary_format::CompiledModule;
use move_core_types::account_address::AccountAddress;
use move_package_alt::{
    package::RootPackage,
    schema::{Environment, EnvironmentName},
};
use move_package_alt_compilation::build_config::BuildConfig as MoveBuildConfig;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use sui_move_build::BuildConfig;
use sui_package_alt::SuiFlavor;
use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    digests::TransactionDigest,
    move_package::{MovePackage, TypeOrigin, UpgradeInfo},
    object::{Data, Object},
    supported_protocol_versions::ProtocolConfig,
};

/// Information about a package in the cache
pub struct PackageInfo {
    node: Node,
    package_id: ObjectID,
}

impl PackageInfo {
    fn new(node: Node, package_id: ObjectID) -> Self {
        Self { node, package_id }
    }

    /// Extract the original package from the file system store
    /// Returns the package object, and version number.
    /// Expects exactly one version file to exist.
    fn extract_original_package(&self) -> Result<(Object, u64)> {
        let (_, version) = self.pkg_version_path()?;

        // Use FileSystemStore to properly deserialize the object
        let store = FileSystemStore::new(self.node.clone())?;
        let key = ObjectKey {
            object_id: self.package_id,
            version_query: VersionQuery::Version(version),
        };

        let objects = store.get_objects(&[key])?;

        // Unwraps are safe because we know there is exactly one package object
        let (pkg, _) = objects.first().unwrap().clone().unwrap();

        Ok((pkg, version))
    }

    fn pkg_dir_path(&self) -> Result<PathBuf> {
        // Get chain ID from node_mapping.csv
        let chain_id =
            get_chain_id_from_mapping(&self.node).context("Failed to get chain ID for node")?;

        Ok(FileSystemStore::base_path()?
            .join(chain_id)
            .join(OBJECTS_DIR)
            .join(self.package_id.to_string()))
    }

    /// Get the implicit package version path and the version number itself.
    /// This is the version corresponding to the name of the package
    /// file in the cache.
    fn pkg_version_path(&self) -> Result<(PathBuf, u64)> {
        let pkg_dir = self.pkg_dir_path()?;

        if !pkg_dir.exists() {
            bail!(
                "Package {} not found in cache at {:?}",
                self.package_id,
                pkg_dir
            );
        }

        // Find numeric files in the directory (these are version files)
        let mut version_files = Vec::new();
        for entry in fs::read_dir(&pkg_dir)? {
            let entry = entry?;
            let filename = entry.file_name();
            let Some(filename_str) = filename.to_str() else {
                continue;
            };

            // Check if the filename is a number (version)
            if let Ok(version) = filename_str.parse::<u64>() {
                version_files.push((entry.path(), version));
            }
        }

        if version_files.is_empty() {
            bail!("No version file found in package directory {:?}", pkg_dir);
        }

        if version_files.len() > 1 {
            bail!(
                "Expected exactly one version file in package directory {:?}",
                pkg_dir,
            );
        }
        Ok(version_files.first().unwrap().clone())
    }

    /// Save a package object to a file
    pub fn save_package_to_file(object: &Object, output_path: &PathBuf) -> Result<()> {
        let bytes = bcs::to_bytes(object).context("Failed to serialize package object to BCS")?;

        // Ensure parent directory exists
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).context("Failed to create output directory")?;
        }

        fs::write(output_path, &bytes).context("Failed to write package binary to disk")?;

        Ok(())
    }

    /// Replace the package in the cache with the given object
    pub fn save_package_to_cache(
        &self,
        object: &Object,
        version: SequenceNumber,
    ) -> Result<PathBuf> {
        let pkg_file = self.pkg_dir_path()?.join(version.value().to_string());

        if !pkg_file.exists() {
            bail!(
                "Package {} version {} not found in cache",
                self.package_id,
                version
            );
        }

        let bytes = bcs::to_bytes(object).context("Failed to serialize package object to BCS")?;

        fs::write(&pkg_file, &bytes).context("Failed to write package to cache")?;

        Ok(pkg_file)
    }
}

/// Rebuilds a package by combining an on-chain package with newly compiled source code
pub struct PackageRebuilder {
    package_info: PackageInfo,
    source_path: PathBuf,
    output_path: Option<PathBuf>,
    env: EnvironmentName,
}

/// Read chain ID from node_mapping.csv file
fn get_chain_id_from_mapping(node: &Node) -> Result<String> {
    let mapping_file = FileSystemStore::base_path()?.join(NODE_MAPPING_FILE);

    if !mapping_file.exists() {
        bail!(
            "Node mapping file not found at {:?}. Please ensure the replay data store is properly initialized.",
            mapping_file
        );
    }

    let file =
        fs::File::open(&mapping_file).context(format!("Failed to open {}", NODE_MAPPING_FILE))?;

    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .trim(csv::Trim::All)
        .from_reader(file);

    let node_name = node.network_name();

    // Read the file and find the matching node
    for result in rdr.records() {
        let record = result.context(format!(
            "Failed to read CSV record from {}",
            NODE_MAPPING_FILE
        ))?;

        if record.len() != 2 {
            bail!(
                "Invalid format in {}. Expected 2 columns (node_name,chain_id), got {}",
                NODE_MAPPING_FILE,
                record.len()
            );
        }

        let name = record[0].trim();
        let chain_id = record[1].trim();

        if name == node_name {
            return Ok(chain_id.to_string());
        }
    }

    bail!("Node '{}' not found in {}", node_name, NODE_MAPPING_FILE)
}

impl PackageRebuilder {
    pub fn new(
        node: Node,
        package_id: ObjectID,
        source_path: PathBuf,
        output_path: Option<PathBuf>,
        env: EnvironmentName,
    ) -> Self {
        Self {
            package_info: PackageInfo::new(node, package_id),
            source_path,
            output_path,
            env,
        }
    }

    /// Main entry point to rebuild a package
    pub fn rebuild(&self) -> Result<()> {
        // Step 1: Extract original package from file system store
        let (original_object, extracted_version) = self.package_info.extract_original_package()?;

        // Step 2: Get metadata from original package
        let (original_package, tx_digest, version) =
            self.extract_package_metadata(&original_object)?;

        if extracted_version != version.value() {
            bail!(
                "Extracted version {} does not actual package match version {}",
                extracted_version,
                version
            );
        }

        // Step 3: Compile the new source code with the original package ID
        let compiled_modules = self.compile_package_with_id()?;

        // Step 4: Reconstruct the package with new modules but original metadata
        let rebuilt_package = self.rebuild_package(original_package, compiled_modules, version)?;

        // Step 5: Create the object wrapper
        let rebuilt_object = Object::new_from_package(rebuilt_package, tx_digest);

        // Step 6: Serialize and save (either to file or cache)
        match &self.output_path {
            Some(output_path) => {
                // Save to specified file
                PackageInfo::save_package_to_file(&rebuilt_object, output_path)?;

                // Step 7: Verify if source unchanged (for testing)
                self.verify_rebuild(&original_object, &rebuilt_object)?;

                println!(
                    "Package rebuilt successfully and saved to: {:?}",
                    output_path
                );
            }
            None => {
                // Replace in cache
                let cache_path = self
                    .package_info
                    .save_package_to_cache(&rebuilt_object, version)?;

                // Step 7: Verify if source unchanged (for testing)
                self.verify_rebuild(&original_object, &rebuilt_object)?;

                println!("Package rebuilt and updated in cache at: {:?}", cache_path);
            }
        }

        Ok(())
    }

    /// Extract metadata from the original package object
    fn extract_package_metadata(
        &self,
        object: &Object,
    ) -> Result<(MovePackage, TransactionDigest, SequenceNumber)> {
        let package = match &object.data {
            Data::Package(pkg) => pkg.clone(),
            _ => bail!("Object {} is not a package", self.package_info.package_id),
        };

        let tx_digest = object.previous_transaction;
        let version = object.version();

        Ok((package, tx_digest, version))
    }

    /// Compile the source package with the original package ID
    fn compile_package_with_id(&self) -> Result<Vec<CompiledModule>> {
        // Create build config (following build.rs pattern)
        let config = MoveBuildConfig::default();

        let envs = RootPackage::<SuiFlavor>::environments(&self.source_path)?;
        let Some(env_id) = envs.get(&self.env) else {
            todo!()
        };
        let environment = Environment {
            name: self.env.clone(),
            id: env_id.clone(),
        };

        // Create BuildConfig - simplified like in build.rs
        let config = BuildConfig {
            config,
            run_bytecode_verifier: false, // We don't need verification for rebuilding
            print_diags_to_stderr: true,  // Print diagnostics like build.rs does
            environment,
        };

        // Build the package (same as build.rs does)
        let compiled_package = config
            .build(&self.source_path)
            .context("Failed to build package")?;

        // Get the compiled modules
        let mut modules = compiled_package.get_modules().cloned().collect::<Vec<_>>();

        // Update the package ID in the compiled modules to match the original
        self.update_module_addresses(&mut modules, self.package_info.package_id)?;

        Ok(modules)
    }

    /// Update the package address in compiled modules to match the target package ID
    fn update_module_addresses(
        &self,
        modules: &mut [CompiledModule],
        target_id: ObjectID,
    ) -> Result<()> {
        let target_address = AccountAddress::from(target_id);

        for module in modules.iter_mut() {
            // Get the self module handle to find which address index it uses
            let self_handle_idx = module.self_module_handle_idx.0 as usize;
            let self_handle = module
                .module_handles
                .get(self_handle_idx)
                .ok_or_else(|| anyhow!("Invalid self module handle index"))?;

            // Get the address index from the module handle
            let address_idx = self_handle.address.0 as usize;

            // Update the address in the address pool only if it's currently 0x0
            if let Some(address) = module.address_identifiers.get_mut(address_idx) {
                // Only replace if the current address is 0x0
                if *address == AccountAddress::ZERO {
                    *address = target_address;
                }
            } else {
                bail!("Invalid address index {} in module", address_idx);
            }
        }

        Ok(())
    }

    /// Build type origin table by merging existing origins with new ones
    /// This follows the same logic as build_upgraded_type_origin_table in sui-types
    fn build_merged_type_origin_table(
        &self,
        original_package: &MovePackage,
        compiled_modules: &[CompiledModule],
    ) -> Result<Vec<TypeOrigin>> {
        let mut new_table = vec![];
        let mut existing_table = original_package.type_origin_map();

        // Process all types in the new compiled modules
        for module in compiled_modules {
            // Process struct definitions
            for struct_def in module.struct_defs() {
                let struct_handle = module.datatype_handle_at(struct_def.struct_handle);
                let module_name = module.name().to_string();
                let struct_name = module.identifier_at(struct_handle.name).to_string();
                let type_key = (module_name.clone(), struct_name.clone());

                // If type exists in original, preserve its origin; otherwise use current package ID
                let origin_package = existing_table
                    .remove(&type_key)
                    .unwrap_or(self.package_info.package_id);
                new_table.push(TypeOrigin {
                    module_name,
                    datatype_name: struct_name,
                    package: origin_package,
                });
            }

            // Process enum definitions
            for enum_def in module.enum_defs() {
                let enum_handle = module.datatype_handle_at(enum_def.enum_handle);
                let module_name = module.name().to_string();
                let enum_name = module.identifier_at(enum_handle.name).to_string();
                let type_key = (module_name.clone(), enum_name.clone());

                // If type exists in original, preserve its origin; otherwise use current package ID
                let origin_package = existing_table
                    .remove(&type_key)
                    .unwrap_or(self.package_info.package_id);
                new_table.push(TypeOrigin {
                    module_name,
                    datatype_name: enum_name,
                    package: origin_package,
                });
            }
        }

        // Check if any types were removed (not allowed for compatibility)
        if !existing_table.is_empty() {
            let removed_types: Vec<String> = existing_table
                .keys()
                .map(|(module, datatype)| format!("{}::{}", module, datatype))
                .collect();
            bail!(
                "Package rebuild would remove the following types, which breaks compatibility: {}",
                removed_types.join(", ")
            );
        }

        Ok(new_table)
    }

    /// Extract immediate dependencies from compiled modules
    fn extract_immediate_dependencies(&self, modules: &[CompiledModule]) -> BTreeSet<ObjectID> {
        let mut immediate_deps = BTreeSet::new();

        for module in modules {
            // Get all immediate dependencies (excluding self)
            for dep in module.immediate_dependencies() {
                let dep_id = ObjectID::from(*dep.address());
                // Don't include self-references
                if dep_id != self.package_info.package_id {
                    immediate_deps.insert(dep_id);
                }
            }
        }

        immediate_deps
    }

    /// Compare dependencies between new modules and original package
    fn compare_dependencies(
        &self,
        new_modules: &[CompiledModule],
        original_package: &MovePackage,
    ) -> Result<()> {
        // Get immediate dependencies from new modules
        let new_deps = self.extract_immediate_dependencies(new_modules);

        // Get immediate dependencies from original package modules
        let mut original_deps = BTreeSet::new();
        for module_bytes in original_package.serialized_module_map().values() {
            // Deserialize original module to extract dependencies
            let original_module = CompiledModule::deserialize_with_defaults(module_bytes)
                .context("Failed to deserialize original package module")?;

            for dep in original_module.immediate_dependencies() {
                let dep_id = ObjectID::from(*dep.address());
                // Don't include self-references
                if dep_id != self.package_info.package_id {
                    original_deps.insert(dep_id);
                }
            }
        }

        // Compare dependency sets
        let added_deps: Vec<_> = new_deps.difference(&original_deps).collect();
        let removed_deps: Vec<_> = original_deps.difference(&new_deps).collect();

        if !added_deps.is_empty() || !removed_deps.is_empty() {
            let mut error_msg = String::from(
                "Dependencies have changed, which is not supported for package rebuild:\n",
            );

            if !added_deps.is_empty() {
                error_msg.push_str(&format!(
                    "  Added dependencies: {}\n",
                    added_deps
                        .iter()
                        .map(|d| format!("0x{}", d))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }

            if !removed_deps.is_empty() {
                error_msg.push_str(&format!(
                    "  Removed dependencies: {}\n",
                    removed_deps
                        .iter()
                        .map(|d| format!("0x{}", d))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }

            error_msg.push_str("Package rebuild requires identical dependencies. ");

            bail!(error_msg);
        }

        Ok(())
    }

    /// Build linkage table by validating dependencies and reusing original
    fn build_new_linkage_table(
        &self,
        modules: &[CompiledModule],
        original_package: &MovePackage,
    ) -> Result<BTreeMap<ObjectID, UpgradeInfo>> {
        // At this point we assume that dependencies would not change
        self.compare_dependencies(modules, original_package)
            .context("Dependency validation failed")?;

        // If dependencies are identical, it's safe to reuse the original linkage table
        let original_linkage = original_package.linkage_table().clone();

        if !original_linkage.is_empty() {
            println!(
                "Reusing original linkage table with {} entries",
                original_linkage.len()
            );
        }

        Ok(original_linkage)
    }

    /// Rebuild the package with new modules but preserving original metadata
    fn rebuild_package(
        &self,
        original_package: MovePackage,
        compiled_modules: Vec<CompiledModule>,
        version: SequenceNumber,
    ) -> Result<MovePackage> {
        // Convert compiled modules to the format needed by MovePackage
        let mut module_map = BTreeMap::new();
        for module in &compiled_modules {
            let module_name = module.name().to_string();
            // Use the proper serialization method for CompiledModule with version
            let mut module_bytes = Vec::new();
            module
                .serialize_with_version(module.version, &mut module_bytes)
                .context("Failed to serialize module")?;
            module_map.insert(module_name, module_bytes);
        }

        // Build type origin table by merging existing origins with new ones
        let type_origin_table = self
            .build_merged_type_origin_table(&original_package, &compiled_modules)
            .context("Failed to build type origin table")?;

        // Build linkage table for dependencies
        let linkage_table = self
            .build_new_linkage_table(&compiled_modules, &original_package)
            .context("Failed to build linkage table")?;

        // Get protocol config for max package size
        let protocol_config = ProtocolConfig::get_for_max_version_UNSAFE();
        let max_package_size = protocol_config.max_move_package_size();

        // Create a new MovePackage with updated modules and properly generated tables
        let rebuilt_package = MovePackage::new(
            self.package_info.package_id, // Use the package ID directly
            version,
            module_map,
            max_package_size,
            type_origin_table,
            linkage_table,
        )
        .context("Failed to create new MovePackage")?;

        Ok(rebuilt_package)
    }

    /// Verify that the rebuilt package matches the original (when source unchanged)
    fn verify_rebuild(&self, original: &Object, rebuilt: &Object) -> Result<()> {
        let original_bytes = bcs::to_bytes(original)?;
        let rebuilt_bytes = bcs::to_bytes(rebuilt)?;

        if original_bytes == rebuilt_bytes {
            println!("✓ Verification passed: Rebuilt package matches the original");
        } else {
            println!("⚠ Warning: Rebuilt package differs from original");
            println!("  Original size: {} bytes", original_bytes.len());
            println!("  Rebuilt size: {} bytes", rebuilt_bytes.len());

            // Extract and compare type origin tables for debugging
            if let (Data::Package(orig_pkg), Data::Package(rebuilt_pkg)) =
                (&original.data, &rebuilt.data)
            {
                println!(
                    "  Original type origins: {} entries",
                    orig_pkg.type_origin_table().len()
                );
                println!(
                    "  Rebuilt type origins: {} entries",
                    rebuilt_pkg.type_origin_table().len()
                );
                println!(
                    "  Original linkage table: {} entries",
                    orig_pkg.linkage_table().len()
                );
                println!(
                    "  Rebuilt linkage table: {} entries",
                    rebuilt_pkg.linkage_table().len()
                );
            }
        }

        Ok(())
    }
}

/// Entry point for the package rebuild command
pub fn rebuild_package(
    node: Node,
    package_id: ObjectID,
    source_path: PathBuf,
    output_path: Option<PathBuf>,
    env: EnvironmentName,
) -> Result<()> {
    let rebuilder = PackageRebuilder::new(node, package_id, source_path, output_path, env);
    rebuilder.rebuild()
}

/// Entry point for the extract-package command
pub fn extract_package(node: Node, package_id: ObjectID, output_path: PathBuf) -> Result<()> {
    // Create PackageInfo to handle extraction
    let package_info = PackageInfo::new(node, package_id);

    // Extract the package using the shared implementation
    let (object, version) = package_info
        .extract_original_package()
        .context("Failed to extract package from cache")?;

    // Save extracted package to the specified output path
    PackageInfo::save_package_to_file(&object, &output_path)
        .context("Failed to save extracted package")?;

    println!(
        "Successfully extracted package {} (version {}) to {}",
        package_id,
        version,
        output_path.display()
    );

    Ok(())
}

/// Entry point for the overwrite-package command
pub fn overwrite_package(node: Node, package_id: ObjectID, package_path: PathBuf) -> Result<()> {
    // Read and validate the package file
    let package_bytes = fs::read(&package_path)
        .context(format!("Failed to read package file: {:?}", package_path))?;

    // Validate it's a valid BCS-encoded Object
    let _object: Object = bcs::from_bytes(&package_bytes)
        .context("Invalid package file: not a valid BCS-encoded Object")?;

    // Create PackageInfo to handle path resolution
    let package_info = PackageInfo::new(node, package_id);

    // Get the implicit package version path and version number
    let (version_path, _) = package_info.pkg_version_path()?;

    // Write the package to the target path
    fs::write(&version_path, &package_bytes).context(format!(
        "Failed to write package to cache: {:?}",
        version_path
    ))?;

    println!(
        "Package {} successfully overwritten in cache at: {:?}",
        package_id, version_path
    );

    Ok(())
}
