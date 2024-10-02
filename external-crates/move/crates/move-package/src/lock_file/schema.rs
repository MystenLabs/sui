// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Serde compatible types to deserialize the schematized parts of the lock file (everything in the
//! [move] table).  This module does not support serialization because of limitations in the `toml`
//! crate related to serializing types as inline tables.

use std::{
    collections::HashMap,
    io::{Read, Seek, Write},
};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;
use toml::value::Value;
use toml_edit::{
    ArrayOfTables,
    Item::{self, Value as EItem},
    Value as EValue,
};

use move_compiler::editions::{Edition, Flavor};

use super::LockFile;

/// Lock file version written by this version of the compiler.  Backwards compatibility is
/// guaranteed (the compiler can read lock files with older versions), forward compatibility is not
/// (the compiler will fail to read lock files at newer versions).
///
/// V0: Base version.
/// V1: Adds toolchain versioning support.
/// V2: Adds support for managing addresses on package publish and upgrades.
/// V3: Renames dependency `name` field to `id` and adds a `name` field to store the name from the manifest.
pub const VERSION: u16 = 3;

/// Table for storing package info under an environment.
const ENV_TABLE_NAME: &str = "env";

/// Table keys in environment for managing published packages.
const ORIGINAL_PUBLISHED_ID_KEY: &str = "original-published-id";
const LATEST_PUBLISHED_ID_KEY: &str = "latest-published-id";
const PUBLISHED_VERSION_KEY: &str = "published-version";
const CHAIN_ID_KEY: &str = "chain-id";

#[derive(Deserialize)]
pub struct Packages {
    #[serde(rename = "package")]
    pub packages: Option<Vec<Package>>,

    #[serde(rename = "dependencies")]
    pub root_dependencies: Option<Vec<Dependency>>,

    #[serde(rename = "dev-dependencies")]
    pub root_dev_dependencies: Option<Vec<Dependency>>,
}

#[derive(Deserialize)]
pub struct Package {
    /// Package identifier (as resolved by the package hook).
    pub id: String,

    /// Where to find this dependency.  Schema is not described in terms of serde-compatible
    /// structs, so it is deserialized into a generic data structure.
    pub source: Value,

    /// The version resolved from the version resolution hook.
    pub version: Option<String>,

    pub dependencies: Option<Vec<Dependency>>,
    #[serde(rename = "dev-dependencies")]
    pub dev_dependencies: Option<Vec<Dependency>>,
}

#[derive(Deserialize)]
pub struct Dependency {
    /// Package identifier (as resolved by the package hook).
    pub id: String,

    /// The name of the dependency (corresponds to the key for the dependency in the depending
    /// package's source manifest).
    pub name: String,

    /// Mappings for named addresses to apply to the package being depended on, when referred to by
    /// the depending package.
    #[serde(rename = "addr_subst")]
    pub subst: Option<Value>,

    /// Expected hash for the source and manifest of the package being depended upon.
    pub digest: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ToolchainVersion {
    /// The Move compiler version used to compile this package.
    #[serde(rename = "compiler-version")]
    pub compiler_version: String,
    /// The Move compiler configuration used to compile this package.
    pub edition: Edition,
    pub flavor: Flavor,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ManagedPackage {
    #[serde(rename = "chain-id")]
    pub chain_id: String,
    #[serde(rename = "original-published-id")]
    pub original_published_id: String,
    #[serde(rename = "latest-published-id")]
    pub latest_published_id: String,
    #[serde(rename = "published-version")]
    pub version: String,
}

#[derive(Serialize, Deserialize)]
pub struct Header {
    pub version: u16,
    /// A hash of the manifest file content this lock file was generated from computed using SHA-256
    /// hashing algorithm.
    pub manifest_digest: String,
    /// A hash of all the dependencies (their lock file content) this lock file depends on, computed
    /// by first hashing all lock files using SHA-256 hashing algorithm and then combining them into
    /// a single digest using SHA-256 hasher (similarly to the package digest is computed). If there
    /// are no dependencies, it's an empty string.
    pub deps_digest: String,
}

#[derive(Serialize, Deserialize)]
struct Schema<T> {
    #[serde(rename = "move")]
    move_: T,
}

impl Packages {
    /// Read packages from the lock file, assuming the file's format matches the schema expected
    /// by this lock file, and its version is not newer than the version supported by this library.
    pub fn read(lock: &mut impl Read) -> Result<(Packages, Header)> {
        let contents = {
            let mut buf = String::new();
            lock.read_to_string(&mut buf).context("Reading lock file")?;
            buf
        };
        let Schema { move_: packages } =
            toml::de::from_str::<Schema<Packages>>(&contents).context("Deserializing packages")?;

        Ok((packages, Header::from_str(&contents)?))
    }
}

impl ToolchainVersion {
    /// Read toolchain version info from the lock file. Returns successfully with None if
    /// parsing the lock file succeeds but an entry for `[toolchain-version]` does not exist.
    pub fn read(lock: &mut impl Read) -> Result<Option<ToolchainVersion>> {
        let contents = {
            let mut buf = String::new();
            lock.read_to_string(&mut buf).context("Reading lock file")?;
            buf
        };

        #[derive(Deserialize)]
        struct TV {
            #[serde(rename = "toolchain-version")]
            toolchain_version: Option<ToolchainVersion>,
        }
        let Schema { move_: value } = toml::de::from_str::<Schema<TV>>(&contents)
            .context("Deserializing toolchain version")?;

        Ok(value.toolchain_version)
    }
}

impl ManagedPackage {
    pub fn read(lock: &mut impl Read) -> Result<HashMap<String, ManagedPackage>> {
        let contents = {
            let mut buf = String::new();
            lock.read_to_string(&mut buf).context("Reading lock file")?;
            buf
        };

        #[derive(Deserialize)]
        struct Lookup {
            env: HashMap<String, ManagedPackage>,
        }
        let Lookup { env } = toml::de::from_str::<Lookup>(&contents)
            .context("Deserializing managed package in environment")?;
        Ok(env)
    }
}

impl Header {
    /// Read lock file header after verifying that the version of the lock is not newer than the version
    /// supported by this library.
    pub fn read(lock: &mut impl Read) -> Result<Header> {
        let contents = {
            let mut buf = String::new();
            lock.read_to_string(&mut buf).context("Reading lock file")?;
            buf
        };
        Self::from_str(&contents)
    }

    fn from_str(contents: &str) -> Result<Header> {
        let Schema { move_: header } =
            toml::de::from_str::<Schema<Header>>(contents).context("Deserializing lock header")?;

        if header.version != VERSION {
            bail!(
                "Lock file format mismatch, expected version {}, found {}",
                VERSION,
                header.version
            );
        }

        Ok(header)
    }
}

/// Write the initial part of the lock file.
pub(crate) fn write_prologue(
    file: &mut NamedTempFile,
    manifest_digest: String,
    deps_digest: String,
) -> Result<()> {
    writeln!(
        file,
        "# @generated by Move, please check-in and do not edit manually.\n"
    )?;

    let prologue = toml::ser::to_string(&Schema {
        move_: Header {
            version: VERSION,
            manifest_digest,
            deps_digest,
        },
    })?;

    write!(file, "{}", prologue)?;
    Ok(())
}

pub fn update_dependency_graph(
    file: &mut LockFile,
    manifest_digest: String,
    deps_digest: String,
    dependencies: Option<toml_edit::Value>,
    dev_dependencies: Option<toml_edit::Value>,
    packages: Option<ArrayOfTables>,
) -> Result<()> {
    use toml_edit::value;
    let mut toml_string = String::new();
    file.read_to_string(&mut toml_string)?;
    let mut toml = toml_string.parse::<toml_edit::Document>()?;
    let move_table = toml
        .entry("move")
        .or_insert(Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .ok_or_else(|| anyhow!("Could not find or create move table in Move.lock"))?;

    // Update `version`, `manifest_digest`, and `deps_digest` in `[move]` table section.
    move_table["version"] = value(VERSION as i64);
    move_table["manifest_digest"] = value(manifest_digest);
    move_table["deps_digest"] = value(deps_digest);

    // Update `dependencies = [ ... ]` in `[move]` table section.
    if let Some(dependencies) = dependencies {
        move_table["dependencies"] = Item::Value(dependencies.clone());
    } else {
        move_table.remove("dependencies");
    }

    // Update `dev-dependencies = [ ... ]` in `[move]` table section.
    if let Some(dev_dependencies) = dev_dependencies {
        move_table["dev-dependencies"] = Item::Value(dev_dependencies.clone());
    } else {
        move_table.remove("dev-dependencies");
    }

    // Update the [[move.package]] Array of Tables.
    if let Some(packages) = packages {
        toml["move"]["package"] = Item::ArrayOfTables(packages.clone());
    } else if let Some(packages_table) = toml["move"]["package"].as_table_mut() {
        packages_table.remove("package");
    }

    file.set_len(0)?;
    file.rewind()?;
    write!(file, "{}", toml)?;
    file.flush()?;
    Ok(())
}

pub fn update_compiler_toolchain(
    file: &mut LockFile,
    compiler_version: String,
    edition: Edition,
    flavor: Flavor,
) -> Result<()> {
    let mut toml_string = String::new();
    file.read_to_string(&mut toml_string)?;
    let mut toml = toml_string.parse::<toml_edit::Document>()?;
    let move_table = toml["move"].as_table_mut().ok_or(std::fmt::Error)?;
    let toolchain_version = toml::Value::try_from(ToolchainVersion {
        compiler_version,
        edition,
        flavor,
    })?;
    move_table["toolchain-version"] = to_toml_edit_value(&toolchain_version);
    file.set_len(0)?;
    file.rewind()?;
    write!(file, "{}", toml)?;
    file.flush()?;
    Ok(())
}

fn to_toml_edit_value(value: &toml::Value) -> toml_edit::Item {
    match value {
        Value::String(v) => EItem(EValue::from(v.clone())),
        Value::Integer(v) => EItem(EValue::from(*v)),
        Value::Float(v) => EItem(EValue::from(*v)),
        Value::Boolean(v) => EItem(EValue::from(*v)),
        Value::Datetime(v) => EItem(EValue::from(v.to_string())),
        Value::Array(arr) => {
            let mut toml_edit_arr = toml_edit::Array::new();
            for x in arr {
                let item = to_toml_edit_value(x);
                match item {
                    EItem(i) => toml_edit_arr.push(i),
                    _ => panic!("cant"),
                }
            }
            EItem(EValue::from(toml_edit_arr))
        }
        Value::Table(table) => {
            let mut toml_edit_table = toml_edit::Table::new();
            for (k, v) in table {
                toml_edit_table[k] = to_toml_edit_value(v);
            }
            toml_edit::Item::Table(toml_edit_table)
        }
    }
}

pub enum ManagedAddressUpdate {
    Published {
        original_id: String,
        chain_id: String,
    },
    Upgraded {
        latest_id: String,
        version: u64,
    },
}

/// Sets the `original-published-id` to a given `id` in the lock file. This is a raw utility
/// for preparing package publishing and package upgrades. Invariant: callers maintain a valid
/// hex `id`.
pub fn set_original_id(file: &mut LockFile, environment: &str, id: &str) -> Result<()> {
    use toml_edit::{value, Document};
    let mut toml_string = String::new();
    file.read_to_string(&mut toml_string)?;
    let mut toml = toml_string.parse::<Document>()?;
    let env_table = toml
        .get_mut(ENV_TABLE_NAME)
        .and_then(|item| item.as_table_mut())
        .ok_or_else(|| anyhow!("Could not find 'env' table in Move.lock"))?
        .get_mut(environment)
        .and_then(|item| item.as_table_mut())
        .ok_or_else(|| anyhow!("Could not find {environment} table in Move.lock"))?;
    env_table[ORIGINAL_PUBLISHED_ID_KEY] = value(id);

    file.set_len(0)?;
    file.rewind()?;
    write!(file, "{}", toml)?;
    file.flush()?;
    file.rewind()?;
    Ok(())
}

/// Saves published or upgraded package addresses in the lock file.
pub fn update_managed_address(
    file: &mut LockFile,
    environment: &str,
    managed_address_update: ManagedAddressUpdate,
) -> Result<()> {
    use toml_edit::{value, Document, Table};

    let mut toml_string = String::new();
    file.read_to_string(&mut toml_string)?;
    let mut toml = toml_string.parse::<Document>()?;

    let env_table = toml
        .entry(ENV_TABLE_NAME)
        .or_insert_with(|| Item::Table(Table::new()))
        .as_table_mut()
        .ok_or_else(|| anyhow!("Could not find or create 'env' table in Move.lock"))?
        .entry(environment)
        .or_insert_with(|| Item::Table(Table::new()))
        .as_table_mut()
        .ok_or_else(|| anyhow!("Could not find or create {environment} table in Move.lock"))?;

    match managed_address_update {
        ManagedAddressUpdate::Published {
            original_id,
            chain_id,
        } => {
            env_table[CHAIN_ID_KEY] = value(chain_id);
            env_table[ORIGINAL_PUBLISHED_ID_KEY] = value(&original_id);
            env_table[LATEST_PUBLISHED_ID_KEY] = value(original_id);
            env_table[PUBLISHED_VERSION_KEY] = value("1");
        }
        ManagedAddressUpdate::Upgraded { latest_id, version } => {
            if !env_table.contains_key(CHAIN_ID_KEY) {
                bail!("Move.lock violation: attempted address update for package upgrade when no {CHAIN_ID_KEY} exists")
            }
            if !env_table.contains_key(ORIGINAL_PUBLISHED_ID_KEY) {
                bail!("Move.lock violation: attempted address update for package upgrade when no {ORIGINAL_PUBLISHED_ID_KEY} exists")
            }
            env_table[LATEST_PUBLISHED_ID_KEY] = value(latest_id);
            env_table[PUBLISHED_VERSION_KEY] = value(version.to_string());
        }
    }

    file.set_len(0)?;
    file.rewind()?;
    write!(file, "{}", toml)?;
    file.flush()?;
    file.rewind()?;
    Ok(())
}
