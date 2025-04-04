// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::read_to_string,
    path::{Path, PathBuf},
};

use anyhow::bail;
use derive_where::derive_where;
use serde::{Deserialize, Serialize};
use serde_spanned::Spanned;
use toml_edit::{
    visit_mut::{visit_table_like_kv_mut, visit_table_mut, VisitMut},
    Document, InlineTable, Item, KeyMut, Table, Value,
};

use crate::{
    dependency::{ManifestDependencyInfo, PinnedDependencyInfo},
    flavor::MoveFlavor,
};

use super::{EnvironmentName, PackageName};

#[derive(Serialize, Deserialize)]
#[derive_where(Clone, Default)]
#[serde(bound = "")]
pub struct Lockfile<F: MoveFlavor> {
    unpublished: UnpublishedTable<F>,

    #[serde(default)]
    published: BTreeMap<EnvironmentName, Publication<F>>,
}

#[derive(Serialize, Deserialize)]
#[derive_where(Clone)]
#[serde(bound = "")]
pub struct Publication<F: MoveFlavor> {
    #[serde(flatten)]
    metadata: F::PublishedMetadata,
    dependencies: BTreeMap<PackageName, PinnedDependencyInfo<F>>,
}

#[derive(Serialize, Deserialize)]
#[derive_where(Default, Clone)]
#[serde(rename_all = "kebab-case")]
#[serde(bound = "")]
struct UnpublishedTable<F: MoveFlavor> {
    dependencies: UnpublishedDependencies<F>,

    #[serde(default)]
    dep_overrides: BTreeMap<EnvironmentName, UnpublishedDependencies<F>>,
}

#[derive(Serialize, Deserialize)]
#[derive_where(Default, Clone)]
#[serde(bound = "")]
struct UnpublishedDependencies<F: MoveFlavor> {
    pinned: BTreeMap<PackageName, PinnedDependencyInfo<F>>,
    unpinned: BTreeMap<PackageName, ManifestDependencyInfo<F>>,
}

impl<F: MoveFlavor> Lockfile<F> {
    /// Read `Move.lock` and all `Move.<env>.lock` files from the directory at `path`.
    /// Returns a new empty [Lockfile] if `path` doesn't contain a `Move.lock`.
    pub fn read_from(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        // Parse `Move.lock`
        let lockfile_name = path.as_ref().join("Move.lock");
        let Ok(lockfile_str) = read_to_string(&lockfile_name) else {
            return Ok(Self::default());
        };
        let mut lockfiles = toml_edit::de::from_str::<Self>(&lockfile_str)?;

        // Add in `Move.<env>.lock` files
        let dir = std::fs::read_dir(path)?;
        for entry in dir {
            let Ok(file) = entry else { continue };

            let Some(env_name) = lockname_to_env_name(file.file_name()) else {
                continue;
            };

            let metadata_contents = std::fs::read_to_string(file.path())?;
            let metadata: Publication<F> = toml_edit::de::from_str(&metadata_contents)
                .or_else(|_| bail!(format!("Couldn't parse {file:?}")))?;

            let old_entry = lockfiles.published.insert(env_name.clone(), metadata);
            if old_entry.is_some() {
                bail!("Move.lock and Move.{env_name}.lock both contain publication information for {env_name}; TODO.");
            }
        }

        Ok(lockfiles)
    }

    /// Serialize [self] into `Move.lock` and `Move.<env>.lock`.
    ///
    /// The [PublishedMetadata] in `self.published.<env>` are partitioned: if `env` is in [envs]
    /// then it is saved to `Move.lock` (and `Move.<env>.lock` is deleted), otherwise the metadata
    /// is stored in `Move.<env>.lock`.
    pub fn write_to(
        &self,
        path: impl AsRef<Path>,
        envs: BTreeMap<EnvironmentName, F::EnvironmentID>,
    ) -> anyhow::Result<()> {
        let mut output: Lockfile<F> = self.clone();
        let (pubs, locals): (BTreeMap<_, _>, BTreeMap<_, _>) = output
            .published
            .into_iter()
            .partition(|(env_name, metadata)| envs.contains_key(env_name));
        output.published = pubs;

        std::fs::write(path.as_ref().join("Move.lock"), output.render())?;

        for (chain, metadata) in locals {
            std::fs::write(
                path.as_ref().join(format!("Move.{}.lock", chain)),
                metadata.render(),
            )?;
        }

        for chain in output.published.keys() {
            let _ = std::fs::remove_file(path.as_ref().join(format!("Move.{}.lock", chain)));
        }

        Ok(())
    }

    /// Pretty-print [self] as a TOML document
    fn render(&self) -> String {
        let mut toml = toml_edit::ser::to_document(self).expect("toml serialization succeeds");

        expand_toml(&mut toml);
        // TODO: maybe this could be more concise and not duplicated in [PublishedMetadata.render]
        // by making the flattener smarter (e.g. it knows to fold anything called pinned, unpinned,
        // or dependencies, or something like that)
        flatten_toml(&mut toml["unpublished"]["dependencies"]["pinned"]);
        flatten_toml(&mut toml["unpublished"]["dependencies"]["unpinned"]);
        flatten_toml(&mut toml["unpublished"]["dependencies"]["unpinned"]);
        for (_, chain) in toml["unpublished"]["dep-overrides"]
            .as_table_like_mut()
            .unwrap()
            .iter_mut()
        {
            flatten_toml(chain.get_mut("pinned").unwrap());
            flatten_toml(chain.get_mut("unpinned").unwrap());
        }

        for (_, chain) in toml["published"].as_table_like_mut().unwrap().iter_mut() {
            flatten_toml(chain.get_mut("dependencies").unwrap());
        }

        toml.decor_mut()
            .set_prefix("# Generated by move; do not edit\n# This file should be checked in.\n\n");

        toml.to_string()
    }
}

impl<F: MoveFlavor> Publication<F> {
    /// Pretty-print [self] as TOML
    fn render(&self) -> String {
        let mut toml = toml_edit::ser::to_document(self).expect("toml serialization succeeds");
        expand_toml(&mut toml);
        flatten_toml(&mut toml["dependencies"]);

        toml.decor_mut().set_prefix(
            "# Generated by move; do not edit\n# This file should not be checked in\n\n",
        );
        toml.to_string()
    }
}

/// Replace every inline table in [toml] with an implicit standard table (implicit tables are not
/// included if they have no keys directly inside them)
fn expand_toml(toml: &mut Document) {
    struct Expander;

    impl VisitMut for Expander {
        fn visit_table_mut(&mut self, table: &mut Table) {
            table.set_implicit(true);
            visit_table_mut(self, table);
        }

        fn visit_table_like_kv_mut(&mut self, mut key: KeyMut<'_>, node: &mut Item) {
            if let Item::Value(Value::InlineTable(inline_table)) = node {
                let inline_table = std::mem::replace(inline_table, InlineTable::new());
                let table = inline_table.into_table();
                key.fmt();
                *node = Item::Table(table);
            }
            visit_table_like_kv_mut(self, key, node);
        }
    }

    let mut visitor = Expander;
    visitor.visit_document_mut(toml);
}

/// Replace every table in [toml] with a non-implicit inline table.
fn flatten_toml(toml: &mut Item) {
    struct Inliner;

    impl VisitMut for Inliner {
        fn visit_table_mut(&mut self, table: &mut Table) {
            table.set_implicit(false);
            visit_table_mut(self, table);
        }

        fn visit_table_like_kv_mut(&mut self, mut key: KeyMut<'_>, node: &mut Item) {
            if let Item::Table(table) = node {
                let table = std::mem::replace(table, Table::new());
                let inline_table = table.into_inline_table();
                key.fmt();
                *node = Item::Value(Value::InlineTable(inline_table));
            }
        }
    }

    let mut visitor = Inliner;
    visitor.visit_item_mut(toml);
}

/// Given a filename of the form `Move.<env>.lock`, returns `<env>`.
fn lockname_to_env_name(filename: OsString) -> Option<String> {
    let Ok(filename) = filename.into_string() else {
        return None;
    };

    let prefix = "Move.";
    let suffix = ".lock";

    if filename.starts_with(prefix) && filename.ends_with(suffix) {
        let start_index = prefix.len();
        let end_index = filename.len() - suffix.len();

        if start_index < end_index {
            return Some(filename[start_index..end_index].to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lockname_to_env_name() {
        assert_eq!(
            lockname_to_env_name(OsString::from("Move.test.lock")),
            Some("test".to_string())
        );
        assert_eq!(
            lockname_to_env_name(OsString::from("Move.3vcga23.lock")),
            Some("3vcga23".to_string())
        );
        assert_eq!(
            lockname_to_env_name(OsString::from("Mve.test.lock.lock")),
            None
        );

        assert_eq!(lockname_to_env_name(OsString::from("Move.lock")), None);
        assert_eq!(lockname_to_env_name(OsString::from("Move.test.loc")), None);
        assert_eq!(lockname_to_env_name(OsString::from("Move.testloc")), None);
        assert_eq!(lockname_to_env_name(OsString::from("Move.test")), None);
    }
}
