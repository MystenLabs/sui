use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::path::Path;
use toml::Value as TV;

use move_core_types::account_address::AccountAddress;
use move_package::source_package::manifest_parser::parse_dependency;
use move_package::source_package::parsed_manifest::{Dependency, PackageName};

const PACKAGES_NAME: &str = "packages";
const CONFIG_NAME: &str = "config";

pub type Packages = BTreeMap<PackageName, Package>;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Config {
    pub rpc: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct OnChainPackage {
    pub id: AccountAddress,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Package {
    Dependency(Dependency),
    OnChain(OnChainPackage),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct GenManifest {
    pub config: Option<Config>,
    pub packages: Packages,
}

pub fn parse_gen_manifest_from_file(path: &Path) -> Result<GenManifest> {
    let file_contents = if path.is_file() {
        std::fs::read_to_string(path)
    } else {
        std::fs::read_to_string(path.join(Path::new("gen.toml")))
    }
    .with_context(|| format!("Unable to find generator manifest at {:?}", path))?;
    parse_gen_manifest(parse_gen_manifest_string(file_contents)?)
}

pub fn parse_gen_manifest_string(manifest_string: String) -> Result<TV> {
    toml::from_str::<TV>(&manifest_string).context("Unable to parse generator manifest")
}

pub fn parse_gen_manifest(tval: TV) -> Result<GenManifest> {
    match tval {
        TV::Table(mut table) => {
            if !table.contains_key(PACKAGES_NAME) {
                bail!("Missing packages key in manifest")
            };

            let config = if table.contains_key(CONFIG_NAME) {
                Some(parse_config(table.remove(CONFIG_NAME).unwrap())?)
            } else {
                None
            };

            let packages = table
                .remove(PACKAGES_NAME)
                .map(parse_packages)
                .transpose()
                .context("Error parsing '[packages]' section of manifest")?
                .unwrap();
            Ok(GenManifest { config, packages })
        }
        x => {
            bail!("Malformed generator manifest {}. Expected a table at top level, but encountered a {}", x, x.type_str())
        }
    }
}

pub fn parse_config(tval: TV) -> Result<Config> {
    match tval {
        TV::Table(table) => {
            let rpc = table
                .get("rpc")
                .and_then(|tval| tval.as_str())
                .map(|s| s.to_string());
            Ok(Config { rpc })
        }
        x => {
            bail!(
                "Malformed section in manifest {}. Expected a table, but encountered a {}",
                x,
                x.type_str()
            )
        }
    }
}

pub fn parse_packages(tval: TV) -> Result<Packages> {
    match tval {
        TV::Table(table) => {
            let mut pkgs = BTreeMap::new();
            for (pkg_name, dep) in table.into_iter() {
                let pkg_name_ident = PackageName::from(pkg_name.clone());
                let dep = parse_package(&pkg_name, dep)?;
                pkgs.insert(pkg_name_ident, dep);
            }
            Ok(pkgs)
        }
        x => {
            bail!(
                "Malformed section in manifest {}. Expected a table, but encountered a {}",
                x,
                x.type_str()
            )
        }
    }
}

pub fn parse_package(pkg_name: &str, tval: TV) -> Result<Package> {
    let Some(table) = tval.as_table() else {
        bail!("Malformed dependency {}", tval);
    };

    if table.contains_key("id") {
        let Some(Ok(id)) = table
            .get("id")
            .unwrap()
            .as_str()
            .map(AccountAddress::from_hex_literal)
        else {
            bail!("Invalid address");
        };
        Ok(Package::OnChain(OnChainPackage { id }))
    } else {
        Ok(Package::Dependency(parse_dependency(pkg_name, tval)?))
    }
}

#[cfg(test)]
mod tests {
    use move_package::source_package::parsed_manifest as PM;

    use super::*;

    #[test]
    fn test_parse_gen_manifest() {
        let manifest_str = r#"
        [config]
        rpc = "https://fullnode.mainnet.sui.io:443"

        [packages]
        deepbook = { id = "0xdee9" }
        amm = { local = "../move/amm" }
        fixture = { local = "../move/fixture" }
        framework = { git = "https://github.com/MystenLabs/sui.git", subdir = "crates/sui-framework/packages/sui-framework", rev = "releases/sui-v1.0.0-release" }
        "#;

        let act =
            parse_gen_manifest(parse_gen_manifest_string(manifest_str.into()).unwrap()).unwrap();

        let exp = GenManifest {
            config: Some(Config {
                rpc: Some("https://fullnode.mainnet.sui.io:443".to_string()),
            }),
            packages: vec![
                (
                    "amm".into(),
                    Package::Dependency(PM::Dependency::Internal(PM::InternalDependency {
                        kind: PM::DependencyKind::Local("../move/amm".into()),
                        subst: None,
                        digest: None,
                        dep_override: false,
                    })),
                ),
                (
                    "deepbook".into(),
                    Package::OnChain(OnChainPackage {
                        id: AccountAddress::from_hex_literal("0xdee9").unwrap(),
                    }),
                ),
                (
                    "fixture".into(),
                    Package::Dependency(PM::Dependency::Internal(PM::InternalDependency {
                        kind: PM::DependencyKind::Local("../move/fixture".into()),
                        subst: None,
                        digest: None,
                        dep_override: false,
                    })),
                ),
                (
                    "framework".into(),
                    Package::Dependency(PM::Dependency::Internal(PM::InternalDependency {
                        kind: PM::DependencyKind::Git(PM::GitInfo {
                            git_url: "https://github.com/MystenLabs/sui.git".into(),
                            git_rev: "releases/sui-v1.0.0-release".into(),
                            subdir: "crates/sui-framework/packages/sui-framework".into(),
                        }),
                        subst: None,
                        digest: None,
                        dep_override: false,
                    })),
                ),
            ]
            .into_iter()
            .collect(),
        };

        assert_eq!(act, exp);
    }
}
