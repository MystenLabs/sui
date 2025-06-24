// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    compatibility::{
        LegacyAddressDeclarations, LegacyBuildInfo, LegacyDevAddressDeclarations,
        LegacySubstOrRename, LegacySubstitution, LegacyVersion, find_module_name_for_package,
        legacy::LegacyPackageInformation, normalize_path,
    },
    dependency::{
        DependencySet, UnpinnedDependencyInfo,
        external::ExternalDependency,
        git::UnpinnedGitDependency,
        local::LocalDependency,
        onchain::{ConstTrue, OnChainDependency},
    },
    errors::{FileHandle, Located, TheFile},
    flavor::MoveFlavor,
    package::{
        PackageName, PublishInformation, PublishedIds,
        layout::SourcePackageLayout,
        lockfile::DependencyInfo,
        manifest::{Manifest, ManifestDependency, PackageMetadata},
    },
};
use anyhow::{Context, Result, anyhow, bail, format_err};
use move_core_types::account_address::{AccountAddress, AccountAddressParseError};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    str::FromStr,
};
use toml::Value as TV;

const EMPTY_ADDR_STR: &str = "_";

/// TODO: Fill in the valid editions for this.
const VALID_EDITIONS: &[&str] = &["2024", "2024.beta", "2024.alpha"];

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

pub fn parse_legacy_manifest_from_file<F: MoveFlavor>(
    toml_path: PathBuf,
) -> Result<(Manifest<F>, LegacyPackageInformation, FileHandle)> {
    let file_contents = std::fs::read_to_string(&toml_path)
        .with_context(|| format!("Unable to find package manifest at {:?}", toml_path))?;

    let file_handle = FileHandle::new(toml_path)?;
    let (manifest, legacy_info) =
        parse_source_manifest(parse_move_manifest_string(file_contents)?, file_handle)?;

    Ok((manifest, legacy_info, file_handle))
}

fn resolve_move_manifest_path(path: &Path) -> PathBuf {
    if path.is_file() {
        path.into()
    } else {
        path.join(SourcePackageLayout::Manifest.path())
    }
}

/// Starting from a path, that could either be a `Move.toml`
/// or a directory containing a `Move.toml`, derive the
/// root path of the package. If it's directory, we keep it as is,
/// if it's a file, we take it's parent.
fn resolve_root_dir(path: &Path) -> Result<PathBuf> {
    if path.is_file() {
        Ok(path
            .parent()
            .expect("A file has to have a parent dir")
            .to_path_buf())
    } else {
        Ok(path.to_path_buf())
    }
}

pub fn parse_move_manifest_string(manifest_string: String) -> Result<TV> {
    toml::from_str::<TV>(&manifest_string).context("Unable to parse Move package manifest")
}

pub fn parse_source_manifest<F: MoveFlavor>(
    tval: TV,
    file_handle: FileHandle,
) -> Result<(Manifest<F>, LegacyPackageInformation)> {
    match tval {
        TV::Table(mut table) => {
            check_for_required_field_names(&table, REQUIRED_FIELDS)
                .context("Error parsing package manifest")?;
            warn_if_unknown_field_names(&table, KNOWN_NAMES);

            let addresses = table
                .remove(ADDRESSES_NAME)
                .map(parse_addresses)
                .transpose()
                .context("Error parsing '[addresses]' section of manifest")?;

            let dev_addresses = table
                .remove(DEV_ADDRESSES_NAME)
                .map(parse_dev_addresses)
                .transpose()
                .context("Error parsing '[dev-addresses]' section of manifest")?;

            let (legacy_name, edition, published_at) = table
                .remove(PACKAGE_NAME)
                .map(parse_package_info)
                .transpose()
                .context("Error parsing '[package]' section of manifest")?
                .unwrap();

            let build = table
                .remove(BUILD_NAME)
                .map(parse_build_info)
                .transpose()
                .context("Error parsing '[build]' section of manifest")?;

            let dependencies = table
                .remove(DEPENDENCY_NAME)
                .map(|tval| parse_dependencies(tval, file_handle.clone()))
                .transpose()
                .context("Error parsing '[dependencies]' section of manifest")?
                .unwrap_or_default();

            let dev_dependencies = table
                .remove(DEV_DEPENDENCY_NAME)
                .map(|tval| parse_dependencies(tval, file_handle.clone()))
                .transpose()
                .context("Error parsing '[dev-dependencies]' section of manifest")?
                .unwrap_or_default();

            if !VALID_EDITIONS.contains(&edition.as_str()) {
                bail!(
                    "Not a valid legacy manifest. Edition must be one of [{}]",
                    VALID_EDITIONS.join(", ")
                );
            }

            // TODO: is there a better way to handle spans here..?
            let edition = Located::new(edition, file_handle.clone(), 0..1);
            let modern_name = get_modern_name(&addresses, file_handle.path())?;
            let new_name = Located::new(modern_name.clone(), file_handle.clone(), 0..1);

            // Gather the original publish information from the manifest, if it's defined on the Toml file.
            let manifest_address_info = if let Some(published_at) = published_at {
                let latest_id = parse_address_literal(&published_at);
                let original_id = addresses
                    .as_ref()
                    .map(|a| a.get(modern_name.as_str()))
                    .flatten()
                    .map(|x| x.clone())
                    .flatten();

                // If we have BOTH the original and latest id, we can create the published ids!
                if let (Ok(latest_id), Some(original_id)) = (latest_id, original_id) {
                    Some(PublishedIds {
                        original_id,
                        latest_id,
                    })
                } else {
                    None
                }
            } else {
                None
            };

            Ok((
                Manifest {
                    package: PackageMetadata {
                        name: new_name,
                        edition,
                        metadata: Default::default(),
                    },
                    environments: BTreeMap::new(),
                    dependencies,
                    dep_replacements: BTreeMap::new(),
                },
                LegacyPackageInformation {
                    incompatible_name: if legacy_name != modern_name.as_str() {
                        Some(legacy_name)
                    } else {
                        None
                    },
                    addresses: addresses.unwrap_or_default(),
                    dev_addresses,
                    // TODO: Fill these in by reading the `lockfile`.
                    environments: None,
                    manifest_address_info,
                },
            ))
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

fn parse_package_info(tval: TV) -> Result<(String, String, Option<String>)> {
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
                .unwrap_or_default();

            Ok((name, edition, published_at))
        }
        x => bail!(
            "Malformed section in manifest {}. Expected a table, but encountered a {}",
            x,
            x.type_str()
        ),
    }
}

fn parse_dependencies(
    tval: TV,
    handle: FileHandle,
) -> Result<BTreeMap<PackageName, ManifestDependency>> {
    match tval {
        TV::Table(table) => {
            let mut deps = BTreeMap::new();

            for (dep_name, dep) in table.into_iter() {
                // TODO: Create a compliant identifier here, as the old free-flowing String can fail.
                let dep_name_ident = PackageName::new(dep_name)?;

                let dep = parse_dependency(dep, handle)?;
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

fn parse_addresses(tval: TV) -> Result<LegacyAddressDeclarations> {
    match tval {
        TV::Table(table) => {
            let mut addresses = BTreeMap::new();
            for (addr_name, entry) in table.into_iter() {
                let ident = addr_name.clone();
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

pub fn parse_dev_addresses(tval: TV) -> Result<LegacyDevAddressDeclarations> {
    match tval {
        TV::Table(table) => {
            let mut addresses = BTreeMap::new();
            for (addr_name, entry) in table.into_iter() {
                let ident = addr_name.clone();
                match entry.as_str() {
                    Some(entry_str) => {
                        if entry_str == EMPTY_ADDR_STR {
                            bail!(
                                "Found uninstantiated named address '{}'. All addresses in the '{}' field must be instantiated.",
                                ident,
                                DEV_ADDRESSES_NAME
                            );
                        } else if addresses
                            .insert(
                                ident.clone(),
                                parse_address_literal(entry_str).context(format!(
                                    "Invalid address '{}' encountered.",
                                    entry_str
                                ))?,
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

fn parse_external_resolver(resolver_val: &TV, handle: FileHandle) -> Result<ExternalDependency> {
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
        // TODO: Figure out how to get the containing file properly
        containing_file: handle,
    })
}

pub fn parse_dependency(mut tval: TV, handle: FileHandle) -> Result<ManifestDependency> {
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
        .and_then(|e| Some(parse_external_resolver(e, handle)))
    {
        return Ok(ManifestDependency {
            dependency_info: UnpinnedDependencyInfo::External(dependency?),
            is_override: dep_override,
            rename_from: None,
        });
    }

    let subst = table
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

            UnpinnedDependencyInfo::Local(LocalDependency {
                local: normalize_path(local, true /* allow_cwd_parent */).unwrap(),
                // TODO (manos): this is wrong -- should fix once I realise what's the expected path here.
                relative_to_parent_dir: handle.path().to_path_buf(),
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

            UnpinnedDependencyInfo::Git(UnpinnedGitDependency {
                repo: git_url.to_string(),
                path: subdir,
                rev: Some(git_rev.to_string()),
            })
        }

        (None, None, None, Some(id)) => {
            let Some(id) = id.as_str() else {
                bail!("ID not a string")
            };

            // TODO: Implement once we have the on-chain deps design.
            UnpinnedDependencyInfo::OnChain(OnChainDependency {
                on_chain: ConstTrue {},
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

    Ok(ManifestDependency {
        dependency_info: result,
        is_override: dep_override,
        rename_from: None,
    })
}

// TODO: Figure out how we deal with this (AND IF we want to deal with this!).
pub fn parse_substitution(tval: TV) -> Result<LegacySubstitution> {
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

fn parse_digest(tval: TV) -> Result<String> {
    let digest_str = tval
        .as_str()
        .ok_or_else(|| format_err!("Invalid package digest"))?;
    Ok(digest_str.to_string())
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
        eprintln!(
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

/// Given the addresses & the package's path, derive the
/// modern styled name. The modern styled name is:
///
/// 1. The `0x0` address, if using the modern environments on lockfiles
/// 2. The `name` modules use inside sources (e.g. `module yy::aa;`)
pub fn get_modern_name(
    addresses: &Option<BTreeMap<String, Option<AccountAddress>>>,
    root_path: &Path,
) -> Result<PackageName> {
    let Some(list) = addresses else {
        bail!("No addresses found in manifest, so the package name could not be determined.");
    };

    // Find all the addresses with 0x0.
    let zero_addresses = list
        .iter()
        .filter(|(_, address)| {
            address.is_some_and(|address| address == AccountAddress::ZERO) || address.is_none()
        })
        .map(|(name, _)| name)
        .collect::<Vec<_>>();

    // If we have multiple, we cannot continue as this is not allowed.
    if zero_addresses.len() > 1 {
        anyhow!(
            "Multiple 0x0 addresses found. This is not allowed. Names found: {:?}",
            zero_addresses
        );
    }

    // If we have a single 0x0 address, we can use it as the name safely.
    if zero_addresses.len() == 1 {
        return Ok(PackageName::new(zero_addresses[0].to_string())?);
    }

    Ok(find_module_name_for_package(&resolve_root_dir(root_path)?)?)
}
