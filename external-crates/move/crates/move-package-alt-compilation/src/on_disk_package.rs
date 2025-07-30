// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::layout::CompiledPackageLayout;
use move_bytecode_source_map::utils::{serialize_to_json, serialize_to_json_string};
use move_command_line_common::files::{
    DEBUG_INFO_EXTENSION, MOVE_BYTECODE_EXTENSION, MOVE_COMPILED_EXTENSION, MOVE_EXTENSION,
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
}
