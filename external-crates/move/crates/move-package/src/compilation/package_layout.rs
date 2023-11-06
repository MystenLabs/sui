// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub enum CompiledPackageLayout {
    BuildInfo,
    Root,
    Dependencies,
    Sources,
    SourceMaps,
    LockFiles,
    CompiledModules,
    CompiledScripts,
    CompiledDocs,
    CompiledABIs,
}

impl CompiledPackageLayout {
    pub fn path(&self) -> &Path {
        let path = match self {
            Self::BuildInfo => "BuildInfo.yaml",
            Self::Root => "build",
            Self::Dependencies => "dependencies",
            Self::Sources => "sources",
            Self::SourceMaps => "source_maps",
            Self::LockFiles => "locks",
            Self::CompiledModules => "bytecode_modules",
            Self::CompiledScripts => "bytecode_scripts",
            Self::CompiledDocs => "docs",
            Self::CompiledABIs => "abis",
        };
        Path::new(path)
    }

    pub fn path_to_file_after_category(path: &Path) -> PathBuf {
        let mut suffix_components = vec![];
        // reverse iterate until Root is found
        for component in path.components().rev() {
            let component_path: &Path = component.as_ref();
            if component_path == Self::Root.path() {
                break;
            }
            suffix_components.push(component);
        }
        // pop root package name
        suffix_components.pop();
        // pop category
        suffix_components.pop();
        // put the components back in order
        suffix_components.reverse();
        // make the path
        suffix_components.into_iter().collect()
    }
}
