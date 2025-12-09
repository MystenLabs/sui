// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    compatibility::{
        LegacyBuildInfo, LegacySubstOrRename, LegacySubstitution, LegacyVersion,
        find_module_name_for_package,
    },
    errors::FileHandle,
    package::paths::PackagePath,
    schema::{
        DefaultDependency, Environment, ExternalDependency, LocalDepInfo, ManifestDependencyInfo,
        ManifestGitDependency, ModeName, OnChainDepInfo, PackageMetadata, PackageName,
        ParsedManifest, PublishAddresses,
    },
};
use anyhow::{Context, Result, anyhow, bail, format_err};

use colored::Colorize as _;
use move_core_types::{account_address::AccountAddress, identifier::Identifier};
use serde_spanned::Spanned;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    str::FromStr,
};
use toml::Value as TV;
use tracing::{debug, warn};

use super::{legacy::LegacyData, legacy_lockfile::load_legacy_lockfile, parse_address_literal};
use move_compiler::editions::Edition;

const EMPTY_ADDR_STR: &str = "_";

/// For packages that do not have a name defined, we are using a predefined name
/// to be able to identify their status.
pub(crate) const NO_NAME_LEGACY_PACKAGE_NAME: &str = "unnamed_legacy_package";

pub const PACKAGE_NAME: &str = "package";
const BUILD_NAME: &str = "build";
const ADDRESSES_NAME: &str = "addresses";
const DEV_ADDRESSES_NAME: &str = "dev-addresses";
const DEPENDENCY_NAME: &str = "dependencies";
const DEV_DEPENDENCY_NAME: &str = "dev-dependencies";

const EXTERNAL_RESOLVER_PREFIX: &str = "r";

const KNOWN_NAMES: &[&str] = &[
    PACKAGE_NAME,
    BUILD_NAME,
    ADDRESSES_NAME,
    DEV_ADDRESSES_NAME,
    DEPENDENCY_NAME,
    DEV_DEPENDENCY_NAME,
    EXTERNAL_RESOLVER_PREFIX,
];

const REQUIRED_FIELDS: &[&str] = &[PACKAGE_NAME];

const LEGACY_SYSTEM_DEPS_NAMES: [&str; 5] =
    ["Sui", "MoveStdlib", "Bridge", "DeepBook", "SuiSystem"];

pub struct LegacyPackageMetadata {
    pub legacy_name: String,
    pub edition: Option<Edition>,
    pub published_at: Option<String>,
    pub unrecognized_fields: BTreeMap<String, toml::Value>,
    pub implicit_deps: bool,
}

/// If `path` contains a valid legacy manifest, convert it to a modern format and return it. By
/// "valid legacy manifest", we mean a manifest that parses correctly and contains at least one of
/// the unsupported sections: `[addresses]`, `[dev-addresses]`, or `[dev-dependencies]`. Although
/// these fields are not technically required in the old system, we want to process manifests that
/// don't have them using the modern parser.
pub fn try_load_legacy_manifest(
    path: &PackagePath,
    default_env: &Environment,
    is_root: bool,
) -> anyhow::Result<Option<(FileHandle, ParsedManifest)>> {
    let Ok(file_handle) = FileHandle::new(path.path().join("Move.toml")) else {
        debug!("failed to load legacy file");
        return Ok(None);
    };

    let Ok(parsed) = parse_move_manifest_string(file_handle.source()) else {
        debug!("failed to parse manifest as toml");
        return Ok(None);
    };

    let TV::Table(ref table) = parsed else {
        debug!("parsed manifest was not a table");
        return Ok(None);
    };

    let has_legacy_fields = [ADDRESSES_NAME, DEV_ADDRESSES_NAME, DEV_DEPENDENCY_NAME]
        .into_iter()
        .any(|key| table.contains_key(key));

    if !has_legacy_fields {
        debug!("manifest didn't have legacy fields");
        return Ok(None);
    }

    debug!("parsing legacy manifest");
    let manifest = parse_source_manifest(parsed, is_root, path, default_env)?;
    debug!("successfully parsed");
    Ok(Some((file_handle, manifest)))
}

fn parse_move_manifest_string(manifest_string: &str) -> Result<TV> {
    toml::from_str::<TV>(manifest_string).context("Unable to parse Move package manifest")
}

fn parse_source_manifest(
    tval: TV,
    is_root: bool,
    path: &PackagePath,
    env: &Environment,
) -> Result<ParsedManifest> {
    match tval {
        TV::Table(mut table) => {
            check_for_required_field_names(&table, REQUIRED_FIELDS)
                .context("Error parsing package manifest")?;
            warn_if_unknown_field_names(&table, KNOWN_NAMES);

            let addresses = table
                .remove(ADDRESSES_NAME)
                .map(parse_addresses)
                .transpose()
                .context("Error parsing '[addresses]' section of manifest")?
                .ok_or_else(|| {
                    anyhow::anyhow!("'[addresses]' section of manifest cannot be empty.")
                })?;

            let metadata = table
                .remove(PACKAGE_NAME)
                .map(parse_package_info)
                .transpose()
                .context("Error parsing '[package]' section of manifest")?
                .unwrap();

            let _build = table
                .remove(BUILD_NAME)
                .map(parse_build_info)
                .transpose()
                .context("Error parsing '[build]' section of manifest")?;

            let mut dependencies = table
                .remove(DEPENDENCY_NAME)
                .map(|deps| parse_dependencies(deps, None))
                .transpose()
                .context("Error parsing '[dependencies]' section of manifest")?
                .unwrap_or_default();

            let dev_dependencies = table
                .remove(DEV_DEPENDENCY_NAME)
                .map(|deps| parse_dependencies(deps, Some("test")))
                .transpose()
                .context("Error parsing '[dev-dependencies]' section of manifest")?
                .unwrap_or_default();

            dependencies.extend(dev_dependencies);

            let modern_name = derive_modern_name(&addresses, path)?
                .unwrap_or(PackageName::new(NO_NAME_LEGACY_PACKAGE_NAME).expect("Cannot fail"));
            let new_name = temporary_spanned(modern_name.clone());

            let original_id = addresses.get(modern_name.as_str()).copied().flatten();

            // Gather the original publish information from the manifest, if it's defined on the Toml file.
            let manifest_address_info =
                get_manifest_address_info(original_id, metadata.published_at)?;

            // remove the "modern" name (address) from the addresses table to avoid duplications
            // Validate that we no longer support `_` addresses for legacy [addresses] sections!
            let mut programmatic_addresses = BTreeMap::new();

            for (name, addr) in addresses {
                // We skip the package base address from the addresses we want to expose
                // as it is now exposed by default.
                if name == modern_name {
                    continue;
                }

                let Some(addr) = addr else {
                    bail!(
                        "Found non instantiated named address `{}` (declared as `_`). All addresses in the `addresses` field must be instantiated.",
                        name
                    );
                };

                programmatic_addresses.insert(name, addr);
            }

            let implicit_dependencies = check_implicits(
                metadata.legacy_name.as_str(),
                is_root,
                &dependencies,
                metadata.implicit_deps,
            );

            // We create a normalized legacy name, to make sure we can always use a package
            // as an Identifier.
            let normalized_legacy_name =
                normalize_legacy_name_to_identifier(metadata.legacy_name.as_str());

            let legacy_publications =
                load_legacy_lockfile(&path.path().join("Move.lock"))?.unwrap_or_default();

            Ok(ParsedManifest {
                package: PackageMetadata {
                    name: new_name,
                    edition: metadata.edition,
                    implicit_dependencies,
                    unrecognized_fields: metadata.unrecognized_fields,
                },

                dependencies: dependencies
                    .into_iter()
                    .map(|(k, v)| (temporary_spanned(k), v))
                    .collect(),

                environments: BTreeMap::from([(
                    temporary_spanned(env.name().clone()),
                    temporary_spanned(env.id().clone()),
                )]),

                legacy_data: Some(LegacyData {
                    legacy_name: metadata.legacy_name,
                    normalized_legacy_name,
                    named_addresses: programmatic_addresses,
                    manifest_address_info,
                    legacy_publications,
                }),
                dep_replacements: BTreeMap::new(),
            })
        }
        x => {
            bail!(
                "Malformed package manifest {}. Expected a table at top level, but encountered a {}",
                x,
                x.type_str()
            )
        }
    }
}

/// Returns true if implicit dependencies should be added. This is true unless either:
///  - implicit_deps_flag is false,
///  - name is a system dep name,
///  - deps or contains a system dep name
fn check_implicits(
    name: &str,
    is_root: bool,
    deps: &BTreeMap<Identifier, DefaultDependency>,
    implicit_deps_flag: bool,
) -> bool {
    if !implicit_deps_flag {
        return false;
    }

    if LEGACY_SYSTEM_DEPS_NAMES.contains(&name) {
        return false;
    }

    let explicit_implicits: Vec<&str> = deps
        .keys()
        .map(|id| id.as_str())
        .filter(|name| LEGACY_SYSTEM_DEPS_NAMES.contains(name))
        .collect();

    if explicit_implicits.is_empty() {
        return true;
    }

    if is_root {
        warn!(
            "[{}] Dependencies on {} are automatically added, but this feature is \
                disabled for your package because you have explicitly included dependencies on {}. Consider \
                removing these dependencies from `Move.toml`.",
            "NOTE".yellow().bold(),
            move_compiler::format_oxford_list!("and", "{}", LEGACY_SYSTEM_DEPS_NAMES),
            move_compiler::format_oxford_list!("and", "{}", explicit_implicits),
        );
    }

    false
}

pub fn parse_package_info(tval: TV) -> Result<LegacyPackageMetadata> {
    match tval {
        TV::Table(mut table) => {
            check_for_required_field_names(&table, &["name"])?;
            let known_names = ["name", "edition", "published-at", "authors", "license"];

            warn_if_unknown_field_names(&table, known_names.as_slice());

            let name = table
                .remove("name")
                .ok_or_else(|| format_err!("'name' is a required field but was not found",))?;

            let name = name
                .as_str()
                .ok_or_else(|| format_err!("Package name must be a string"))?;

            let published_at = table
                .remove("published-at")
                .map(|v| v.as_str().unwrap_or_default().to_string());

            let implicit_deps = table
                .remove("implicit-dependencies")
                .map(|v| v.as_bool().unwrap_or(true))
                .unwrap_or(true);

            let name = name.to_string();

            // TODO: Decide if we want to add an author list in the new system!

            // let authors = match table.remove("authors") {
            //     None => Vec::new(),
            //     Some(arr) => {
            //         let unparsed_vec = arr
            //             .as_array()
            //             .ok_or_else(|| format_err!("Invalid author(s) list"))?;
            //         unparsed_vec
            //             .iter()
            //             .map(|tval| {
            //                 tval.as_str().map(|x| x.to_string()).ok_or_else(|| {
            //                     format_err!(
            //                         "Invalid author '{}' of type {} found. Expected a string.",
            //                         tval.to_string(),
            //                         tval.type_str()
            //                     )
            //                 })
            //             })
            //             .collect::<Result<_>>()?
            //     }
            // };

            let edition = table
                .remove("edition")
                .map(|v| {
                    let s = v
                        .as_str()
                        .ok_or_else(|| format_err!("'edition' must be a string"))?;
                    Edition::from_str(s).map_err(|err| format_err!("Invalid 'edition'. {err}"))
                })
                .transpose()?;

            Ok(LegacyPackageMetadata {
                legacy_name: name.clone(),
                edition,
                published_at,
                unrecognized_fields: table.into_iter().collect(),
                implicit_deps,
            })
        }
        x => bail!(
            "Malformed section in manifest {}. Expected a table, but encountered a {}",
            x,
            x.type_str()
        ),
    }
}

/// Given a "legacy" string, we produce an Identifier that is as "consistent"
/// as possible.
fn normalize_legacy_name_to_identifier(name: &str) -> Identifier {
    // rules for `Identifier`:
    //  - all characters must be a-z, A-z, 0-9, or `_`
    //  - first character is not a digit
    //  - entire string is not `_`
    //  - string is non-empty

    let mut result = String::new();

    for c in name.chars() {
        result.push(if c.is_ascii_alphanumeric() { c } else { '_' });
    }

    if result.is_empty() || result == "_" {
        return Identifier::new("__").expect("__ is a valid identifier");
    }

    if result.chars().next().unwrap().is_numeric() {
        result.insert(0, '_');
    }

    Identifier::new(result).expect("tranformed string is a valid identifier")
}

fn parse_dependencies(
    tval: TV,
    mode: Option<&str>,
) -> Result<BTreeMap<PackageName, DefaultDependency>> {
    match tval {
        TV::Table(table) => {
            let mut deps = BTreeMap::new();

            for (dep_name, dep) in table.into_iter() {
                let dep_name_ident = normalize_legacy_name_to_identifier(&dep_name);
                let dep = parse_dependency(dep, mode)?;
                deps.insert(dep_name_ident, dep);
            }

            Ok(deps)
        }
        x => bail!(
            "Malformed section in manifest {}. Expected a table, but encountered a {}",
            x,
            x.type_str()
        ),
    }
}

fn parse_build_info(tval: TV) -> Result<LegacyBuildInfo> {
    match tval {
        TV::Table(mut table) => {
            warn_if_unknown_field_names(&table, &["language_version", "arch"]);
            Ok(LegacyBuildInfo {
                language_version: table
                    .remove("language_version")
                    .map(parse_version)
                    .transpose()?,
            })
        }
        x => bail!(
            "Malformed section in manifest {}. Expected a table, but encountered a {}",
            x,
            x.type_str()
        ),
    }
}

fn parse_addresses(tval: TV) -> Result<BTreeMap<Identifier, Option<AccountAddress>>> {
    match tval {
        TV::Table(table) => {
            let mut addresses = BTreeMap::new();
            for (addr_name, entry) in table.into_iter() {
                let ident = Identifier::new(addr_name)?;

                match entry.as_str() {
                    Some(entry_str) => {
                        if entry_str == EMPTY_ADDR_STR {
                            if addresses.insert(ident.clone(), None).is_some() {
                                bail!("Duplicate address name '{}' found.", ident);
                            }
                        } else if addresses
                            .insert(
                                ident.clone(),
                                Some(parse_address_literal(entry_str).context(format!(
                                    "Invalid address '{}' encountered.",
                                    entry_str
                                ))?),
                            )
                            .is_some()
                        {
                            bail!("Duplicate address name '{}' found.", ident);
                        }
                    }
                    None => bail!(
                        "Invalid address name {} encountered. Expected a string but found a {}",
                        entry,
                        entry.type_str()
                    ),
                }
            }
            Ok(addresses)
        }
        x => bail!(
            "Malformed section in manifest {}. Expected a table, but encountered a {}",
            x,
            x.type_str()
        ),
    }
}

fn parse_external_resolver(resolver_val: &TV) -> Result<ExternalDependency> {
    let Some(table) = resolver_val.as_table() else {
        bail!("Malformed dependency {}", resolver_val);
    };

    if table.len() != 1 {
        bail!(
            "Malformed external resolver declaration for dependency {EXTERNAL_RESOLVER_PREFIX}.{resolver_val}",
        );
    }

    let key = table
        .keys()
        .next()
        .expect("Exactly one key by check above")
        .as_str();

    let key_value = table.get(key).ok_or_else(|| {
        format_err!("Malformed external resolver declaration for dependency {EXTERNAL_RESOLVER_PREFIX}.{resolver_val}",)
    })?;

    if !key_value.is_str() {
        bail!(
            "Malformed external resolver value for dependency {EXTERNAL_RESOLVER_PREFIX}.{resolver_val}"
        );
    }

    // We parse the old dependencies using the new style rgardless.
    Ok(ExternalDependency {
        resolver: key.to_string(),
        data: key_value.clone(),
    })
}

fn parse_dependency(mut tval: TV, mode: Option<&str>) -> Result<DefaultDependency> {
    let Some(table) = tval.as_table_mut() else {
        bail!("Malformed dependency {}", tval);
    };

    let modes = mode.map(|mode| [ModeName::from(mode)].into());

    let dep_override = table
        .remove("override")
        .map(parse_dep_override)
        .transpose()?
        .is_some_and(|o| o);

    if let Some(dependency) = table
        .get(EXTERNAL_RESOLVER_PREFIX)
        .map(parse_external_resolver)
    {
        return Ok(DefaultDependency {
            dependency_info: ManifestDependencyInfo::External(dependency?),
            is_override: dep_override,
            rename_from: None,
            modes,
        });
    }

    let _subst = table
        .remove("addr_subst")
        .map(parse_substitution)
        .transpose()?;

    let result = match (
        table.remove("local"),
        table.remove("subdir"),
        table.remove("git"),
        table.remove("id"),
    ) {
        (Some(local), subdir, None, None) => {
            if subdir.is_some() {
                bail!("'subdir' not supported for local dependencies");
            }

            let Some(local) = local.as_str().map(PathBuf::from) else {
                bail!("Local source path not a string")
            };

            ManifestDependencyInfo::Local(LocalDepInfo {
                local: local.to_path_buf(),
            })
        }

        (None, subdir, Some(git_url), None) => {
            let Some(git_rev) = table.remove("rev") else {
                bail!("Git revision not supplied for dependency")
            };

            let Some(git_rev) = git_rev.as_str() else {
                bail!("Git revision not a string")
            };

            let Some(git_url) = git_url.as_str() else {
                bail!("Git URL not a string")
            };

            let subdir = match subdir {
                None => PathBuf::new(),
                Some(path) => path
                    .as_str()
                    .map(PathBuf::from)
                    .ok_or_else(|| anyhow!("'subdir' not a string"))?,
            };

            ManifestDependencyInfo::Git(ManifestGitDependency {
                repo: git_url.to_string(),
                subdir,
                rev: Some(git_rev.to_string()),
            })
        }

        (None, None, None, Some(id)) => {
            let Some(_id) = id.as_str() else {
                bail!("ID not a string")
            };

            // TODO: Implement once we have the on-chain deps design.
            ManifestDependencyInfo::OnChain(OnChainDepInfo {
                on_chain: true.try_into().unwrap(),
            })
        }

        _ => {
            let keys = ["'local'", "'git'", "'r.<external_resolver_binary_name>'"];
            bail!(
                "must provide exactly one of {} for dependency.",
                keys.join(" or ")
            )
        }
    };

    // Any fields that are left are unknown
    warn_if_unknown_field_names(table, &[]);

    Ok(DefaultDependency {
        dependency_info: result,
        is_override: dep_override,
        rename_from: None,
        modes,
    })
}

// TODO: Figure out how we deal with this (and IF we want to deal with this).
fn parse_substitution(tval: TV) -> Result<LegacySubstitution> {
    match tval {
        TV::Table(table) => {
            let mut subst = BTreeMap::new();
            for (addr_name, tval) in table.into_iter() {
                let addr_ident = addr_name.clone();
                match tval {
                    TV::String(addr_or_name) => {
                        if let Ok(addr) = AccountAddress::from_hex_literal(&addr_or_name) {
                            subst.insert(addr_ident, LegacySubstOrRename::Assign(addr));
                        } else {
                            let rename_from = addr_or_name.as_str();
                            subst.insert(
                                addr_ident,
                                LegacySubstOrRename::RenameFrom(rename_from.to_string()),
                            );
                        }
                    }
                    x => bail!(
                        "Malformed dependency substitution {}. Expected a string, but encountered a {}",
                        x,
                        x.type_str()
                    ),
                }
            }
            Ok(subst)
        }
        x => bail!(
            "Malformed dependency substitution {}. Expected a table, but encountered a {}",
            x,
            x.type_str()
        ),
    }
}

fn parse_version(tval: TV) -> Result<LegacyVersion> {
    let version_str = tval.as_str().unwrap();
    let version_parts = version_str.split('.').collect::<Vec<_>>();
    if version_parts.len() != 3 {
        bail!(
            "Version is malformed. Versions must be of the form <u64>.<u64>.<u64>, but found '{}'",
            version_str
        );
    }

    Ok((
        version_parts[0]
            .parse::<u64>()
            .context("Invalid major version")?,
        version_parts[1]
            .parse::<u64>()
            .context("Invalid minor version")?,
        version_parts[2]
            .parse::<u64>()
            .context("Invalid bugfix version")?,
    ))
}

fn parse_dep_override(tval: TV) -> Result<bool> {
    if !tval.is_bool() {
        bail!("Invalid dependency override value");
    }
    Ok(tval.as_bool().unwrap())
}

// Check that only recognized names are provided at the top-level.
fn warn_if_unknown_field_names(table: &toml::map::Map<String, TV>, known_names: &[&str]) {
    let mut unknown_names = BTreeSet::new();
    for key in table.keys() {
        if !known_names.contains(&key.as_str()) {
            unknown_names.insert(key.to_string());
        }
    }

    if !unknown_names.is_empty() {
        // TODO: manos - to fix this when migration work starts
        tracing::debug!(
            "Warning: unknown field name{} found. Expected one of [{}], but found {}",
            if unknown_names.len() > 1 { "s" } else { "" },
            known_names.join(", "),
            unknown_names
                .into_iter()
                .map(|x| format!("'{}'", x))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
}

fn check_for_required_field_names(
    table: &toml::map::Map<String, TV>,
    required_fields: &[&str],
) -> Result<()> {
    let mut missing_fields = BTreeSet::new();

    for field_name in required_fields {
        if !table.contains_key(*field_name) {
            missing_fields.insert(field_name.to_string());
        }
    }

    if !missing_fields.is_empty() {
        bail!(
            "Required field name{} {} not found",
            if missing_fields.len() > 1 { "s" } else { "" },
            missing_fields
                .into_iter()
                .map(|x| format!("'{}'", x))
                .collect::<Vec<_>>()
                .join(", "),
        )
    }

    Ok(())
}

/// This will be removed (or not) depending on whether we have different types
/// for in-memory implementations. In any way, we cannot offer the same level of support
/// for legacy manifest files.
fn temporary_spanned<T>(val: T) -> Spanned<T> {
    Spanned::new(0..1, val)
}

/// Given the original_id (optional) and the `published_at` from the manifest,
/// we derive the `PublishAddresses`
fn get_manifest_address_info(
    original_id: Option<AccountAddress>,
    published_at: Option<String>,
) -> Result<Option<PublishAddresses>> {
    // 1. If we have published-at, but not original, we return None
    // 2. If we have original, we use that as the address, as long as it is not 0x0
    // 3. If we have neither, we return None
    // 4. If we have both, we split them accordingly.
    match (published_at, original_id) {
        (Some(_), None) => Ok(None),
        (None, None) => Ok(None),
        (None, Some(original_id)) => {
            if original_id == AccountAddress::ZERO {
                return Ok(None);
            }
            Ok(Some(PublishAddresses {
                published_at: crate::schema::PublishedID(original_id),
                original_id: crate::schema::OriginalID(original_id),
            }))
        }
        (Some(published_at), Some(original_id)) => {
            if original_id == AccountAddress::ZERO {
                return Ok(None);
            }
            let published_at = parse_address_literal(&published_at)?;
            Ok(Some(PublishAddresses {
                published_at: crate::schema::PublishedID(published_at),
                original_id: crate::schema::OriginalID(original_id),
            }))
        }
    }
}

/// Given the addresses & the package's path, derive the
/// modern styled name. The modern styled name is:
///
/// 1. The `0x0` address, if using the modern environments on lockfiles
/// 2. The `name` modules use inside sources (e.g. `module yy::aa;`)
fn derive_modern_name(
    addresses: &BTreeMap<Identifier, Option<AccountAddress>>,
    path: &PackagePath,
) -> Result<Option<PackageName>> {
    debug!("Address to derve modern name from: {:?}", addresses);
    // Find all the addresses with 0x0.
    let zero_addresses = addresses
        .iter()
        .filter(|(_, address)| {
            address.is_some_and(|address| address == AccountAddress::ZERO) || address.is_none()
        })
        .map(|(name, _)| name)
        .collect::<Vec<_>>();

    // If we have a single 0x0 address, we can use it as the name safely.
    if zero_addresses.len() == 1 {
        Ok(Some(PackageName::new(zero_addresses[0].to_string())?))
    } else {
        find_module_name_for_package(path)
    }
}

#[cfg(test)]
mod tests {
    use crate::schema::{OriginalID, PublishedID};

    use super::*;

    #[test]
    fn test_get_manifest_address_info() {
        let original_id = Some(AccountAddress::from_hex_literal("0x1").unwrap());
        let published_at = Some("0x2".to_string());
        let manifest_address_info = get_manifest_address_info(original_id, published_at).unwrap();
        assert_eq!(
            manifest_address_info,
            Some(PublishAddresses {
                published_at: PublishedID(AccountAddress::from_hex_literal("0x2").unwrap()),
                original_id: OriginalID(AccountAddress::from_hex_literal("0x1").unwrap()),
            })
        );
    }

    #[test]
    fn test_get_manifest_address_info_no_published_at() {
        let original_id = Some(AccountAddress::from_hex_literal("0x1").unwrap());
        let published_at = None;
        let manifest_address_info = get_manifest_address_info(original_id, published_at).unwrap();
        assert_eq!(
            manifest_address_info,
            Some(PublishAddresses {
                published_at: PublishedID(AccountAddress::from_hex_literal("0x1").unwrap()),
                original_id: OriginalID(AccountAddress::from_hex_literal("0x1").unwrap()),
            })
        );
    }

    #[test]
    fn test_get_manifest_address_info_none_original_id() {
        let original_id = None;
        let published_at = Some("0x2".to_string());
        let result = get_manifest_address_info(original_id, published_at);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn test_get_manifest_address_info_zero_original_id_no_published_at() {
        let original_id = Some(AccountAddress::ZERO);
        let published_at = None;
        let manifest_address_info = get_manifest_address_info(original_id, published_at).unwrap();
        assert_eq!(manifest_address_info, None);
    }

    #[test]
    fn test_get_manifest_address_info_zero_original_id_with_published_at() {
        let original_id = Some(AccountAddress::ZERO);
        let published_at = Some("0x2".to_string());
        let result = get_manifest_address_info(original_id, published_at);

        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn test_get_manifest_address_info_invalid_published_at_format() {
        let original_id = Some(AccountAddress::from_hex_literal("0x1").unwrap());
        let published_at = Some("invalid_address".to_string());
        let result = get_manifest_address_info(original_id, published_at);
        assert!(result.is_err());
    }

    #[test]
    fn normalize_legacy_names() {
        let names = vec![
            ("foo", "foo"),
            ("foo-bar", "foo_bar"),
            ("foo bar", "foo_bar"),
            ("is_normal", "is_normal"),
            ("0x1234", "_0x1234"),
            ("UNO!", "UNO_"),
            ("!", "__"),
        ];

        for (name, expected) in names {
            let identifier = normalize_legacy_name_to_identifier(name);
            assert_eq!(identifier.to_string(), expected);
        }
    }
}
