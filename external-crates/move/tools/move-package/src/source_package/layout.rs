// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

/// References file for documentation generation
pub const REFERENCE_TEMPLATE_FILENAME: &str = "references.md";

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum SourcePackageLayout {
    Sources,
    Specifications,
    Tests,
    Scripts,
    Examples,
    Manifest,
    DocTemplates,
}

impl SourcePackageLayout {
    /// A Move source package is laid out on-disk as
    /// a_move_package
    /// ├── Move.toml      (required)
    /// ├── sources        (required)
    /// ├── examples       (optional, dev mode)
    /// ├── scripts        (optional)
    /// ├── specifications (optional)
    /// ├── doc_templates      (optional)
    /// └── tests          (optional, test mode)
    pub fn path(&self) -> &Path {
        Path::new(self.location_str())
    }

    pub fn try_find_root(starting_path: &Path) -> Result<PathBuf> {
        let mut current_path = starting_path.to_path_buf();
        loop {
            if current_path.join(Self::Manifest.path()).is_file() {
                break Ok(current_path);
            }
            if !current_path.pop() {
                bail!(
                    "Unable to find package manifest in '{}' or in its parents",
                    starting_path.to_string_lossy()
                )
            }
        }
    }

    pub fn location_str(&self) -> &'static str {
        match self {
            Self::Sources => "sources",
            Self::Manifest => "Move.toml",
            Self::Tests => "tests",
            Self::Scripts => "scripts",
            Self::Examples => "examples",
            Self::Specifications => "specifications",
            Self::DocTemplates => "doc_templates",
        }
    }

    pub fn is_optional(&self) -> bool {
        match self {
            Self::Sources | Self::Manifest => false,
            Self::Tests
            | Self::Scripts
            | Self::Examples
            | Self::Specifications
            | Self::DocTemplates => true,
        }
    }
}
