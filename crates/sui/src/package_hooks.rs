use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::{
    fs,
    sync::{Arc, Mutex},
};

use anyhow::Context;
use move_binary_format::{access::ModuleAccess, CompiledModule};
use move_core_types::account_address::AccountAddress;
use move_package::resolution::local_path;
use move_package::source_package::parsed_manifest::DependencyKind;
use move_package::{
    package_hooks::PackageHooks,
    source_package::{
        layout::SourcePackageLayout,
        parsed_manifest::{OnChainInfo, SourceManifest},
    },
};
use move_symbol_pool::Symbol;
use sui_move_build::PUBLISHED_AT_MANIFEST_FIELD;
use sui_sdk::wallet_context::WalletContext;
use sui_sdk::SuiClientBuilder;
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::{
    DEEPBOOK_PACKAGE_ID, MOVE_STDLIB_PACKAGE_ID, SUI_FRAMEWORK_PACKAGE_ID, SUI_SYSTEM_PACKAGE_ID,
};

use crate::package_cache::PackageCache;

struct Dependency {
    original_id: ObjectID,
    published_at: ObjectID,
}

pub struct SuiPackageHooks {
    package_cache: Arc<Mutex<PackageCache>>,
}

impl SuiPackageHooks {
    pub fn new(package_cache: Arc<Mutex<PackageCache>>) -> Self {
        Self { package_cache }
    }

    pub async fn register_from_ctx(ctx: &WalletContext) -> anyhow::Result<()> {
        let rpc_url = &ctx.config.get_active_env()?.rpc;

        let client = SuiClientBuilder::default().build(rpc_url).await?;
        move_package::package_hooks::register_package_hooks(Box::new(SuiPackageHooks::new(
            Arc::new(Mutex::new(PackageCache::new(client))),
        )));

        Ok(())
    }

    async fn resolve_original_id_and_version(
        &self,
        id: ObjectID,
    ) -> anyhow::Result<(AccountAddress, SequenceNumber)> {
        let mut cache = self.package_cache.lock().unwrap();
        let pkg = cache.get(id).await?;
        let module =
            CompiledModule::deserialize_with_defaults(pkg.module_map.first_key_value().unwrap().1)
                .with_context(|| format!("Failed to resolve original-id for: {}", id))?;
        Ok((module.address().clone(), pkg.version))
    }

    async fn resolve_original_id(&self, id: ObjectID) -> anyhow::Result<AccountAddress> {
        let (original_id, _) = self.resolve_original_id_and_version(id).await?;
        Ok(original_id)
    }

    async fn resolve_version(&self, id: ObjectID) -> anyhow::Result<SequenceNumber> {
        let (_, version) = self.resolve_original_id_and_version(id).await?;
        Ok(version)
    }
}

impl PackageHooks for SuiPackageHooks {
    fn custom_package_info_fields(&self) -> Vec<String> {
        vec![PUBLISHED_AT_MANIFEST_FIELD.to_string()]
    }

    fn resolve_on_chain_dependency(
        &self,
        dep_name: move_symbol_pool::Symbol,
        info: &OnChainInfo,
    ) -> anyhow::Result<()> {
        let out_path = local_path(&DependencyKind::OnChain(info.clone()));
        if out_path.exists() {
            fs::remove_dir_all(&out_path)?;
        }

        let id = ObjectID::from_hex_literal(&info.id)
            .with_context(|| format!("Parsing dependency ID: {}", info.id))?;
        let pkg = futures::executor::block_on(async {
            self.package_cache.lock().unwrap().get(id).await
        })?;

        // get direct dependencies
        let all_deps_orig = pkg.linkage_table.keys().cloned().collect::<BTreeSet<_>>();
        let module_deps_orig = pkg
            .module_map
            .values()
            .flat_map(|module| {
                let module = CompiledModule::deserialize_with_defaults(module).unwrap();
                module
                    .immediate_dependencies()
                    .into_iter()
                    .map(|dep| ObjectID::from(*dep.address()))
                    .collect::<BTreeSet<_>>()
            })
            .collect::<BTreeSet<_>>();
        let direct_deps = all_deps_orig
            .intersection(&module_deps_orig)
            .map(|id| Dependency {
                original_id: *id,
                published_at: pkg.linkage_table.get(id).unwrap().upgraded_id,
            })
            .collect::<Vec<_>>();

        // generate a manifest
        fs::create_dir_all(&out_path)?;
        fs::create_dir_all(&out_path.join(SourcePackageLayout::Sources.location_str()))?;
        let mut manifest = format!(
            "[package]\n\
            name = \"{}\"\n\
            published-at = \"{}\"\n",
            dep_name,
            info.id.as_str()
        );

        if !direct_deps.is_empty() {
            writeln!(manifest, "\n[dependencies]").unwrap()
        }
        for dep in direct_deps {
            writeln!(
                manifest,
                "{} = {{ id = \"{}\" }}",
                dep.original_id.to_hex_literal(),
                dep.published_at.to_hex_literal()
            )
            .unwrap();
        }

        match id {
            MOVE_STDLIB_PACKAGE_ID => {
                writeln!(manifest, "\n[addresses]").unwrap();
                writeln!(manifest, "std = \"0x1\"").unwrap();
            }
            SUI_FRAMEWORK_PACKAGE_ID => {
                writeln!(manifest, "\n[addresses]").unwrap();
                writeln!(manifest, "sui = \"0x2\"").unwrap();
            }
            SUI_SYSTEM_PACKAGE_ID => {
                writeln!(manifest, "\n[addresses]").unwrap();
                writeln!(manifest, "sui_system = \"0x3\"").unwrap();
            }
            DEEPBOOK_PACKAGE_ID => {
                writeln!(manifest, "\n[addresses]").unwrap();
                writeln!(manifest, "deepbook = \"0xdee9\"").unwrap();
            }
            _ => (),
        }

        fs::write(out_path.join("Move.toml"), manifest)?;

        // save modules into `build/<dep_name>/`
        let modules_out = out_path
            .join("build")
            .join(dep_name.as_str())
            .join("bytecode_modules");
        fs::create_dir_all(&modules_out)?;

        for (name, bytes) in pkg.module_map {
            fs::write(modules_out.join(format!("{}.mv", name)), &bytes)?;
        }

        // pre-fetch transitive deps to cache
        let all_deps_upgraded = pkg
            .linkage_table
            .values()
            .map(|info| info.upgraded_id)
            .collect::<Vec<_>>();
        futures::executor::block_on(async {
            self.package_cache
                .lock()
                .unwrap()
                .get_multi(all_deps_upgraded)
                .await
        })
        .ok();

        Ok(())
    }

    fn custom_resolve_pkg_name(&self, manifest: &SourceManifest) -> anyhow::Result<Symbol> {
        let published_at = manifest
            .package
            .custom_properties
            .get(&Symbol::from("published-at"));
        match published_at {
            Some(published_at) => {
                let original_id = futures::executor::block_on(async {
                    self.resolve_original_id(
                        ObjectID::from_hex_literal(published_at.as_str()).with_context(|| {
                            format!("Parsing published-at field ID: {}", published_at.as_str())
                        })?,
                    )
                    .await
                })?;
                Ok(Symbol::from(original_id.to_hex_literal()))
            }
            None => Ok(manifest.package.name),
        }
    }

    fn resolve_version(
        &self,
        manifest: &move_package::source_package::parsed_manifest::SourceManifest,
    ) -> anyhow::Result<Option<move_symbol_pool::Symbol>> {
        let published_at = manifest
            .package
            .custom_properties
            .get(&Symbol::from("published-at"));
        match published_at {
            Some(published_at) => {
                let version = futures::executor::block_on(async {
                    self.resolve_version(
                        ObjectID::from_hex_literal(published_at.as_str()).with_context(|| {
                            format!("Parsing published-at field ID: {}", published_at.as_str())
                        })?,
                    )
                    .await
                })?;
                Ok(Some(Symbol::from(version.value().to_string())))
            }
            None => Ok(None),
        }
    }
}
