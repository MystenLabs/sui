// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{DEFAULT_BUILD_DIR, DEFAULT_STORAGE_DIR};
use anyhow::{anyhow, bail, Result};
use move_binary_format::file_format::{CompiledModule, FunctionDefinitionIndex};
use move_bytecode_utils::module_cache::GetModule;
use move_command_line_common::files::MOVE_COMPILED_EXTENSION;
use move_core_types::{
    account_address::AccountAddress,
    identifier::Identifier,
    language_storage::{ModuleId, StructTag},
    resolver::{LinkageResolver, ModuleResolver, ResourceResolver},
};
use move_disassembler::disassembler::Disassembler;
use move_ir_types::location::Spanned;
use std::{
    fs,
    path::{Path, PathBuf},
};

/// subdirectory of `DEFAULT_STORAGE_DIR`/<addr> where modules are stored
pub const MODULES_DIR: &str = "modules";

/// file under `DEFAULT_BUILD_DIR` where a registry of generated struct layouts are stored
pub const STRUCT_LAYOUTS_FILE: &str = "struct_layouts.yaml";

#[derive(Debug)]
pub struct OnDiskStateView {
    build_dir: PathBuf,
    storage_dir: PathBuf,
}

impl OnDiskStateView {
    /// Create an `OnDiskStateView` that reads/writes resource data and modules in `storage_dir`.
    pub fn create<P: Into<PathBuf>>(build_dir: P, storage_dir: P) -> Result<Self> {
        let build_dir = build_dir.into();
        if !build_dir.exists() {
            fs::create_dir_all(&build_dir)?;
        }

        let storage_dir = storage_dir.into();
        if !storage_dir.exists() {
            fs::create_dir_all(&storage_dir)?;
        }

        Ok(Self {
            build_dir,
            // it is important to canonicalize the path here because `is_data_path()` relies on the
            // fact that storage_dir is canonicalized.
            storage_dir: storage_dir.canonicalize()?,
        })
    }

    pub fn build_dir(&self) -> &PathBuf {
        &self.build_dir
    }

    pub fn struct_layouts_file(&self) -> PathBuf {
        self.build_dir.join(STRUCT_LAYOUTS_FILE)
    }

    fn is_data_path(&self, p: &Path, parent_dir: &str) -> bool {
        if !p.exists() {
            return false;
        }
        let p = p.canonicalize().unwrap();
        p.starts_with(&self.storage_dir)
            && match p.parent() {
                Some(parent) => parent.ends_with(parent_dir),
                None => false,
            }
    }

    pub fn is_module_path(&self, p: &Path) -> bool {
        self.is_data_path(p, MODULES_DIR)
    }

    fn get_addr_path(&self, addr: &AccountAddress) -> PathBuf {
        let mut path = self.storage_dir.clone();
        path.push(format!("0x{}", addr));
        path
    }

    fn get_module_path(&self, module_id: &ModuleId) -> PathBuf {
        let mut path = self.get_addr_path(module_id.address());
        path.push(MODULES_DIR);
        path.push(module_id.name().to_string());
        path.with_extension(MOVE_COMPILED_EXTENSION)
    }

    /// Extract a module ID from a path
    pub fn get_module_id(&self, p: &Path) -> Option<ModuleId> {
        if !self.is_module_path(p) {
            return None;
        }
        let name = Identifier::new(p.file_stem().unwrap().to_str().unwrap()).unwrap();
        match p.parent().and_then(|parent| parent.parent()) {
            Some(parent) => {
                let addr =
                    AccountAddress::from_hex_literal(parent.file_stem().unwrap().to_str().unwrap())
                        .unwrap();
                Some(ModuleId::new(addr, name))
            }
            None => None,
        }
    }

    /// Read the resource bytes stored on-disk at `addr`/`tag`
    fn get_module_bytes(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>> {
        Self::get_bytes(&self.get_module_path(module_id))
    }

    /// Check if a module at `addr`/`module_id` exists
    pub fn has_module(&self, module_id: &ModuleId) -> bool {
        self.get_module_path(module_id).exists()
    }

    /// Return the name of the function at `idx` in `module_id`
    pub fn resolve_function(&self, module_id: &ModuleId, idx: u16) -> Result<Option<Identifier>> {
        if let Some(m) = self.get_module_by_id(module_id)? {
            Ok(Some(
                m.identifier_at(
                    m.function_handle_at(m.function_def_at(FunctionDefinitionIndex(idx)).function)
                        .name,
                )
                .to_owned(),
            ))
        } else {
            Ok(None)
        }
    }

    fn get_bytes(path: &Path) -> Result<Option<Vec<u8>>> {
        Ok(if path.exists() {
            Some(fs::read(path)?)
        } else {
            None
        })
    }

    fn view_bytecode(path: &Path) -> Result<Option<String>> {
        if path.is_dir() {
            bail!("Bad bytecode path {:?}. Needed file, found directory", path)
        }

        Ok(match Self::get_bytes(path)? {
            Some(bytes) => {
                let module = CompiledModule::deserialize_with_defaults(&bytes)
                    .map_err(|e| anyhow!("Failure deserializing module: {:?}", e))?;
                // TODO: find or create source map and pass it to disassembler
                let d: Disassembler =
                    Disassembler::from_module(&module, Spanned::unsafe_no_loc(()).loc)?;
                Some(d.disassemble()?)
            }
            None => None,
        })
    }

    pub fn view_module(module_path: &Path) -> Result<Option<String>> {
        Self::view_bytecode(module_path)
    }

    /// Save `module` on disk under the path `module.address()`/`module.name()`
    pub fn save_module(&self, module_id: &ModuleId, module_bytes: &[u8]) -> Result<()> {
        let path = self.get_module_path(module_id);
        if !path.exists() {
            fs::create_dir_all(path.parent().unwrap())?
        }
        Ok(fs::write(path, module_bytes)?)
    }

    /// Save the YAML encoding `layout` on disk under `build_dir/layouts/id`.
    pub fn save_struct_layouts(&self, layouts: &str) -> Result<()> {
        let layouts_file = self.struct_layouts_file();
        if !layouts_file.exists() {
            fs::create_dir_all(layouts_file.parent().unwrap())?
        }
        Ok(fs::write(layouts_file, layouts)?)
    }

    /// Save all the modules in the local cache, re-generate mv_interfaces if required.
    pub fn save_modules<'a>(
        &self,
        modules: impl IntoIterator<Item = &'a (ModuleId, Vec<u8>)>,
    ) -> Result<()> {
        for (module_id, module_bytes) in modules {
            self.save_module(module_id, module_bytes)?;
        }
        Ok(())
    }

    pub fn delete_module(&self, id: &ModuleId) -> Result<()> {
        let path = self.get_module_path(id);
        fs::remove_file(path)?;

        // delete addr directory if this address is now empty
        let addr_path = self.get_addr_path(id.address());
        if addr_path.read_dir()?.next().is_none() {
            fs::remove_dir(addr_path)?
        }
        Ok(())
    }

    fn iter_paths<F>(&self, f: F) -> impl Iterator<Item = PathBuf>
    where
        F: FnOnce(&Path) -> bool + Copy,
    {
        walkdir::WalkDir::new(&self.storage_dir)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
            .map(|e| e.path().to_path_buf())
            .filter(move |path| f(path))
    }

    pub fn module_paths(&self) -> impl Iterator<Item = PathBuf> + '_ {
        self.iter_paths(move |p| self.is_module_path(p))
    }

    /// Build all modules in the self.storage_dir.
    /// Returns an Err if a module does not deserialize.
    pub fn get_all_modules(&self) -> Result<Vec<CompiledModule>> {
        self.module_paths()
            .map(|path| {
                CompiledModule::deserialize_with_defaults(&Self::get_bytes(&path)?.unwrap())
                    .map_err(|e| anyhow!("Failed to deserialized module: {:?}", e))
            })
            .collect::<Result<Vec<CompiledModule>>>()
    }
}

/// Default implementation of LinkageResolver (no re-linking).
impl LinkageResolver for OnDiskStateView {
    type Error = anyhow::Error;
}

impl ModuleResolver for OnDiskStateView {
    type Error = anyhow::Error;
    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        self.get_module_bytes(module_id)
    }
}

impl ResourceResolver for OnDiskStateView {
    type Error = anyhow::Error;

    fn get_resource(
        &self,
        _address: &AccountAddress,
        _struct_tag: &StructTag,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        unimplemented!()
    }
}

impl Default for OnDiskStateView {
    fn default() -> Self {
        OnDiskStateView::create(Path::new(DEFAULT_BUILD_DIR), Path::new(DEFAULT_STORAGE_DIR))
            .expect("Failure creating OnDiskStateView")
    }
}
