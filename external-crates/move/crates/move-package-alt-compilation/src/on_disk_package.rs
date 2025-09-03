// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::ensure;
use move_binary_format::CompiledModule;
use move_compiler::{
    compiled_unit::NamedCompiledModule,
    shared::{
        NumericalAddress,
        files::{FileName, MappedFiles},
    },
};
use serde::{Deserialize, Serialize};

use crate::{compiled_package::CompiledPackage, layout::CompiledPackageLayout};
use move_bytecode_source_map::utils::{
    serialize_to_json, serialize_to_json_string, source_map_from_file,
};
use move_command_line_common::files::{
    DEBUG_INFO_EXTENSION, FileHash, MOVE_BYTECODE_EXTENSION, MOVE_COMPILED_EXTENSION,
    MOVE_EXTENSION, extension_equals, find_filenames, try_exists,
};
use move_disassembler::disassembler::Disassembler;
use move_symbol_pool::Symbol;

use super::compiled_package::{CompiledPackageInfo, CompiledUnitWithSource};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnDiskPackage {
    /// Information about the package and the specific compilation that was done.
    pub compiled_package_info: CompiledPackageInfo,
    /// Dependency names for this package.
    pub dependencies: Vec<Symbol>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnDiskCompiledPackage {
    /// Path to the root of the package and its data on disk. Relative to/rooted at the directory
    /// containing the `Move.toml` file for this package.
    pub root_path: PathBuf,
    pub package: OnDiskPackage,
}

impl OnDiskCompiledPackage {
    pub fn from_path(p: &Path) -> anyhow::Result<Self> {
        let (buf, build_path) = if try_exists(p)? && extension_equals(p, "yaml") {
            (std::fs::read(p)?, p.parent().unwrap().parent().unwrap())
        } else {
            (
                std::fs::read(p.join(CompiledPackageLayout::BuildInfo.path()))?,
                p.parent().unwrap(),
            )
        };
        let package = serde_yaml::from_slice::<OnDiskPackage>(&buf)?;
        assert!(build_path.ends_with(CompiledPackageLayout::Root.path()));
        let root_path = build_path.join(package.compiled_package_info.package_name.as_str());
        Ok(Self { root_path, package })
    }

    pub fn into_compiled_package(&self) -> anyhow::Result<CompiledPackage> {
        let root_name = self.package.compiled_package_info.package_name;
        let mut file_map = MappedFiles::empty();

        assert!(self.root_path.ends_with(root_name.as_str()));
        let root_compiled_units = self.get_compiled_units_paths(root_name)?;
        let root_compiled_units = root_compiled_units
            .into_iter()
            .map(|bytecode_path| self.decode_unit(root_name, &bytecode_path))
            .collect::<anyhow::Result<Vec<_>>>()?;
        let mut deps_compiled_units = vec![];
        for dep_name in self.package.dependencies.iter().copied() {
            let compiled_units = self.get_compiled_units_paths(dep_name)?;
            for bytecode_path in compiled_units {
                deps_compiled_units.push((dep_name, self.decode_unit(dep_name, &bytecode_path)?))
            }
        }

        for unit in root_compiled_units
            .iter()
            .chain(deps_compiled_units.iter().map(|(_, unit)| unit))
        {
            let contents = Arc::from(std::fs::read_to_string(&unit.source_path)?);
            file_map.add(
                FileHash::new(&contents),
                FileName::from(unit.source_path.to_string_lossy().to_string()),
                contents,
            );
        }

        let docs_path = self
            .root_path
            .join(self.package.compiled_package_info.package_name.as_str())
            .join(CompiledPackageLayout::CompiledDocs.path());
        let compiled_docs = if docs_path.is_dir() {
            Some(
                find_filenames(&[docs_path.to_string_lossy().to_string()], |path| {
                    extension_equals(path, "md")
                })?
                .into_iter()
                .map(|path| {
                    let contents = std::fs::read_to_string(&path).unwrap();
                    (path, contents)
                })
                .collect(),
            )
        } else {
            None
        };

        Ok(CompiledPackage {
            compiled_package_info: self.package.compiled_package_info.clone(),
            root_compiled_units,
            deps_compiled_units,
            compiled_docs,
            file_map,
            // TODO: Let's remove this from CompiledPackage
            deps_published_ids: vec![],
        })
    }

    fn decode_unit(
        &self,
        package_name: Symbol,
        bytecode_path_str: &str,
    ) -> anyhow::Result<CompiledUnitWithSource> {
        let package_name_opt = Some(package_name);
        let bytecode_path = Path::new(bytecode_path_str);
        let path_to_file = CompiledPackageLayout::path_to_file_after_category(bytecode_path);
        let bytecode_bytes = std::fs::read(bytecode_path)?;
        let source_map = source_map_from_file(
            &self
                .root_path
                .join(CompiledPackageLayout::DebugInfo.path())
                .join(&path_to_file)
                .with_extension(DEBUG_INFO_EXTENSION),
        )?;
        let source_path = self
            .root_path
            .join(CompiledPackageLayout::Sources.path())
            .join(path_to_file)
            .with_extension(MOVE_EXTENSION);
        ensure!(
            source_path.is_file(),
            "Error decoding package: {}. \
            Unable to find corresponding source file for '{}' in package {}",
            self.package.compiled_package_info.package_name,
            bytecode_path_str,
            package_name
        );
        let module = CompiledModule::deserialize_with_defaults(&bytecode_bytes)?;
        let (address_bytes, module_name) = {
            let id = module.self_id();
            let parsed_addr = NumericalAddress::new(
                id.address().into_bytes(),
                move_compiler::shared::NumberFormat::Hex,
            );
            let module_name = FileName::from(id.name().as_str());
            (parsed_addr, module_name)
        };
        let unit = NamedCompiledModule {
            package_name: package_name_opt,
            address: address_bytes,
            name: module_name,
            module,
            source_map,
            address_name: None,
        };
        Ok(CompiledUnitWithSource { unit, source_path })
    }

    /// Save `bytes` under `path_under` relative to the package on disk
    pub(crate) fn save_under(&self, file: impl AsRef<Path>, bytes: &[u8]) -> anyhow::Result<()> {
        let path_to_save = self.root_path.join(file);
        let parent = path_to_save.parent().unwrap();
        fs::create_dir_all(parent)?;
        fs::write(path_to_save, bytes).map_err(|err| err.into())
    }

    pub(crate) fn save_disassembly_to_disk(
        &self,
        package_name: Symbol,
        unit: &CompiledUnitWithSource,
    ) -> anyhow::Result<()> {
        let root_package = self.package.compiled_package_info.package_name;
        assert!(self.root_path.ends_with(root_package.as_str()));
        let disassembly_dir = CompiledPackageLayout::Disassembly.path();
        let file_path = if root_package == package_name {
            PathBuf::new()
        } else {
            CompiledPackageLayout::Dependencies
                .path()
                .join(package_name.as_str())
        }
        .join(unit.unit.name.as_str());
        let d = Disassembler::from_unit(&unit.unit);
        let (disassembled_string, mut bytecode_map) = d.disassemble_with_source_map()?;
        let disassembly_file_path = disassembly_dir
            .join(&file_path)
            .with_extension(MOVE_BYTECODE_EXTENSION);
        self.save_under(
            disassembly_file_path.clone(),
            disassembled_string.as_bytes(),
        )?;
        // unwrap below is safe as we just successfully saved a file at disassembly_file_path
        if let Ok(p) =
            dunce::canonicalize(self.root_path.join(disassembly_file_path).parent().unwrap())
        {
            bytecode_map
                .set_from_file_path(p.join(&file_path).with_extension(MOVE_BYTECODE_EXTENSION));
        }
        self.save_under(
            disassembly_dir.join(&file_path).with_extension("json"),
            serialize_to_json_string(&bytecode_map)?.as_bytes(),
        )
    }

    pub(crate) fn save_compiled_unit(
        &self,
        package_name: Symbol,
        compiled_unit: &CompiledUnitWithSource,
    ) -> anyhow::Result<()> {
        let root_package = &self.package.compiled_package_info.package_name;
        // assert!(self.root_path.ends_with(root_package.as_str()));
        let category_dir = CompiledPackageLayout::CompiledModules.path();
        let root_pkg_name: Symbol = root_package.as_str().into();
        let file_path = if root_pkg_name == package_name {
            PathBuf::new()
        } else {
            CompiledPackageLayout::Dependencies
                .path()
                .join(package_name.as_str())
        }
        .join(compiled_unit.unit.name.as_str());

        self.save_under(
            category_dir
                .join(&file_path)
                .with_extension(MOVE_COMPILED_EXTENSION),
            compiled_unit.unit.serialize().as_slice(),
        )?;
        self.save_under(
            CompiledPackageLayout::DebugInfo
                .path()
                .join(&file_path)
                .with_extension(DEBUG_INFO_EXTENSION),
            compiled_unit.unit.serialize_source_map().as_slice(),
        )?;
        self.save_under(
            CompiledPackageLayout::DebugInfo
                .path()
                .join(&file_path)
                .with_extension("json"),
            &serialize_to_json(&compiled_unit.unit.source_map)?,
        )?;
        self.save_under(
            CompiledPackageLayout::Sources
                .path()
                .join(&file_path)
                .with_extension(MOVE_EXTENSION),
            fs::read_to_string(&compiled_unit.source_path)?.as_bytes(),
        )
    }

    fn get_compiled_units_paths(&self, package_name: Symbol) -> anyhow::Result<Vec<String>> {
        let package_dir = if self.package.compiled_package_info.package_name == package_name {
            self.root_path.clone()
        } else {
            self.root_path
                .join(CompiledPackageLayout::Dependencies.path())
                .join(package_name.as_str())
        };
        let mut compiled_unit_paths = vec![];
        let module_path = package_dir.join(CompiledPackageLayout::CompiledModules.path());
        if try_exists(&module_path)? {
            compiled_unit_paths.push(module_path);
        }
        find_filenames(&compiled_unit_paths, |path| {
            extension_equals(path, MOVE_COMPILED_EXTENSION)
        })
    }
}
