// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Serde compatible types to deserialize the schematized parts of the lock file (everything in the
//! [move] table).  This module does not support serialization because of limitations in the `toml`
//! crate related to serializing types as inline tables.

use std::io::{Read, Seek, Write};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;
use toml::value::Value;
use toml_edit::{Item::Value as EItem, Value as EValue};

use move_compiler::editions::{Edition, Flavor};

use super::LockFile;

/// Lock file version written by this version of the compiler.  Backwards compatibility is
/// guaranteed (the compiler can read lock files with older versions), forward compatibility is not
/// (the compiler will fail to read lock files at newer versions).
///
/// TODO(amnn): Set to version 1 when stabilised.
pub const VERSION: u64 = 0;

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
    /// The name of the package (corresponds to the name field from its source manifest).
    pub name: String,

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

#[derive(Serialize, Deserialize)]
pub struct Header {
    pub version: u64,
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

        if header.version > VERSION {
            bail!(
                "Lock file format is too new, expected version {} or below, found {}",
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
