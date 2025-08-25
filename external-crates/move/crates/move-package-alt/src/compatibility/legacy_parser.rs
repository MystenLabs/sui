// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    compatibility::{
        LegacyBuildInfo, LegacySubstOrRename, LegacySubstitution, LegacyVersion,
        find_module_name_for_package,
    },
    errors::FileHandle,
    package::{EnvironmentName, paths::PackagePath},
    schema::{
        DefaultDependency, Environment, ExternalDependency, LocalDepInfo, ManifestDependencyInfo,
        ManifestGitDependency, OnChainDepInfo, OriginalID, PackageMetadata, PackageName,
        ParsedManifest, PublishAddresses, PublishedID,
    },
};
use anyhow::{Context, Result, anyhow, bail, format_err};
use move_core_types::{
    account_address::{AccountAddress, AccountAddressParseError},
    identifier::Identifier,
};
use serde_spanned::Spanned;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};
use toml::Value as TV;
use tracing::debug;

use super::legacy::{LegacyData, LegacyEnvironment};

const EMPTY_ADDR_STR: &str = "_";

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
    pub edition: String,
    pub published_at: Option<String>,
    pub unrecognized_fields: BTreeMap<String, toml::Value>,
}

/// If `path` contains a valid legacy manifest, convert it to a modern format and return it. By
/// "valid legacy manifest", we mean a manifest that parses correctly and contains at least one of
/// the unsupported sections: `[addresses]`, `[dev-addresses]`, or `[dev-dependencies]`. Although
/// these fields are not technically required in the old system, we want to process manifests that
/// don't have them using the modern parser.
pub fn try_load_legacy(
    path: &PackagePath,
    default_env: &Environment,
) -> Option<(FileHandle, ParsedManifest)> {
    let Ok(file_handle) = FileHandle::new(path.manifest_path()) else {
        return None;
    };

    let Ok(parsed) = parse_move_manifest_string(file_handle.source()) else {
        return None;
    };

    let TV::Table(ref table) = parsed else {
        return None;
    };

    let has_legacy_fields = [ADDRESSES_NAME, DEV_ADDRESSES_NAME, DEV_DEPENDENCY_NAME]
        .into_iter()
        .any(|key| table.contains_key(key));

    if !has_legacy_fields {
        return None;
    }

    parse_source_manifest(parsed, path, default_env)
        .ok()
        .map(|parsed| (file_handle, parsed))
}

fn parse_legacy_lockfile_addresses(
    path: &PackagePath,
) -> Result<BTreeMap<EnvironmentName, LegacyEnvironment>> {
    // we do not want to error if the lockfile does not exist.
    let file_contents = std::fs::read_to_string(path.lockfile_path())?;

    let toml_val = toml::from_str::<TV>(&file_contents)?;

    let Some(lockfile) = toml_val.as_table() else {
        bail!(
            "Lockfile is malformed. Expected a table at the top level, but found a {}",
            file_contents
        );
    };

    let mut publish_info = BTreeMap::new();

    // Extract the environments as a table.
    let Some(envs) = lockfile.get("env").and_then(|v| v.as_table()) else {
        return Ok(publish_info);
    };

    for (name, data) in envs {
        let env_name = name.to_string();
        let env_table = data.as_table().unwrap();

        let chain_id = env_table
            .get("chain-id")
            .map(|v| v.as_str().unwrap_or_default().to_string());
        let original_id = env_table
            .get("original-published-id")
            .map(|v| parse_address_literal(v.as_str().unwrap_or_default()).unwrap());
        let latest_id = env_table
            .get("latest-published-id")
            .map(|v| parse_address_literal(v.as_str().unwrap_or_default()).unwrap());

        let published_version = env_table
            .get("published-version")
            .map(|v| v.as_str().unwrap_or_default().to_string());

        if let (Some(chain_id), Some(original_id), Some(latest_id), Some(published_version)) =
            (chain_id, original_id, latest_id, published_version)
        {
            publish_info.insert(
                env_name,
                LegacyEnvironment {
                    addresses: PublishAddresses {
                        original_id: OriginalID(original_id),
                        published_at: PublishedID(latest_id),
                    },
                    chain_id,
                    version: published_version,
                },
            );
        }
    }

    Ok(publish_info)
}

fn parse_move_manifest_string(manifest_string: &str) -> Result<TV> {
    toml::from_str::<TV>(manifest_string).context("Unable to parse Move package manifest")
}

fn parse_source_manifest(
    tval: TV,
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

            let dependencies = table
                .remove(DEPENDENCY_NAME)
                .map(parse_dependencies)
                .transpose()
                .context("Error parsing '[dependencies]' section of manifest")?
                .unwrap_or_default();

            let _dev_dependencies = table
                .remove(DEV_DEPENDENCY_NAME)
                .map(parse_dependencies)
                .transpose()
                .context("Error parsing '[dev-dependencies]' section of manifest")?
                .unwrap_or_default();

            let modern_name = derive_modern_name(&addresses, path)?;
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
                        "Found uninstantiated named address `{}` (declared as `_`). All addresses in the `addresses` field must be instantiated.",
                        name
                    );
                };

                programmatic_addresses.insert(name, addr);
            }

            // Check if the package has any system package on its deps.
            let has_system_package = dependencies
                .iter()
                .any(|(name, _)| LEGACY_SYSTEM_DEPS_NAMES.contains(&name.as_str()));

            // Check if the name of the package refers to a system package
            let is_system_package =
                LEGACY_SYSTEM_DEPS_NAMES.contains(&metadata.legacy_name.as_str());

            // IF we have one system package OR this package is a system package itself,
            // we disable implicit deps.
            let system_dependencies = if has_system_package || is_system_package {
                Some(vec![])
            } else {
                None
            };

            Ok(ParsedManifest {
                package: PackageMetadata {
                    name: new_name,
                    edition: metadata.edition,
                    system_dependencies,
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
                    named_addresses: programmatic_addresses,
                    manifest_address_info,
                    legacy_environments: parse_legacy_lockfile_addresses(path).unwrap_or_default(),
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

fn parse_package_info(tval: TV) -> Result<LegacyPackageMetadata> {
    match tval {
        TV::Table(mut table) => {
            check_for_required_field_names(&table, &["name"])?;
            let known_names = ["name", "edition", "published-at"];

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
                .map(|v| v.as_str().unwrap_or_default().to_string())
                .unwrap_or("legacy".to_string());

            Ok(LegacyPackageMetadata {
                legacy_name: name,
                edition,
                published_at,
                unrecognized_fields: table.into_iter().collect(),
            })
        }
        x => bail!(
            "Malformed section in manifest {}. Expected a table, but encountered a {}",
            x,
            x.type_str()
        ),
    }
}

fn parse_dependencies(tval: TV) -> Result<BTreeMap<PackageName, DefaultDependency>> {
    match tval {
        TV::Table(table) => {
            let mut deps = BTreeMap::new();

            for (dep_name, dep) in table.into_iter() {
                // TODO(manos): This could fail if we have names that are not `Identifier` compatible.
                // Though this is a super rare case, we'll probably not handle it more complex until we need to.
                // TODO: we need to support whitespace and decide if that's how we need to keep
                // this working
                let dep_name = dep_name.replace("-", "___");
                let dep_name_ident = PackageName::new(dep_name)?;
                let dep = parse_dependency(dep)?;
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

// Safely parses address for both the 0x and non prefixed hex format.
fn parse_address_literal(address_str: &str) -> Result<AccountAddress, AccountAddressParseError> {
    if !address_str.starts_with("0x") {
        return AccountAddress::from_hex(address_str);
    }
    AccountAddress::from_hex_literal(address_str)
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

fn parse_dependency(mut tval: TV) -> Result<DefaultDependency> {
    let Some(table) = tval.as_table_mut() else {
        bail!("Malformed dependency {}", tval);
    };

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
    // If we have a published-at address, we must have an original-id set (if it's 0x0, we cannot derive it).
    if published_at.is_some()
        && (original_id.is_none() || original_id.is_some_and(|id| id == AccountAddress::ZERO))
    {
        bail!("If `published-at` is defined in Move.toml, `original-id` must also be defined.");
    }

    let Some(original_id) = original_id else {
        return Ok(None);
    };

    // We cannot support "0x0" as the "original-id" of a published package.
    if original_id == AccountAddress::ZERO {
        return Ok(None);
    }

    if let Some(published_at) = published_at {
        let published_at = parse_address_literal(&published_at)?;
        Ok(Some(PublishAddresses {
            published_at: crate::schema::PublishedID(published_at),
            original_id: crate::schema::OriginalID(original_id),
        }))
    } else {
        Ok(Some(PublishAddresses {
            published_at: crate::schema::PublishedID(original_id),
            original_id: crate::schema::OriginalID(original_id),
        }))
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
) -> Result<PackageName> {
    debug!("Address to derve modern name from: {:?}", addresses);
    // Find all the addresses with 0x0.
    let zero_addresses = addresses
        .iter()
        .filter(|(_, address)| {
            address.is_some_and(|address| address == AccountAddress::ZERO) || address.is_none()
        })
        .map(|(name, _)| name)
        .collect::<Vec<_>>();

    // If we have multiple, we cannot continue as this is not allowed.
    if zero_addresses.len() > 1 {
        bail!(
            "Multiple 0x0 addresses found. This is not allowed. Duplicate names found: {:?}",
            zero_addresses
        );
    }

    // If we have a single 0x0 address, we can use it as the name safely.
    if zero_addresses.len() == 1 {
        Ok(PackageName::new(zero_addresses[0].to_string())?)
    } else {
        find_module_name_for_package(path)
    }
}

#[cfg(test)]
mod tests {
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
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains(
            "If `published-at` is defined in Move.toml, `original-id` must also be defined."
        ));
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
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains(
            "If `published-at` is defined in Move.toml, `original-id` must also be defined."
        ));
    }

    #[test]
    fn test_get_manifest_address_info_invalid_published_at_format() {
        let original_id = Some(AccountAddress::from_hex_literal("0x1").unwrap());
        let published_at = Some("invalid_address".to_string());
        let result = get_manifest_address_info(original_id, published_at);
        assert!(result.is_err());
    }
}
