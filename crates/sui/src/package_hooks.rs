use anyhow::{Context, Result};
use move_binary_format::CompiledModule;
use move_compiler::editions::Edition;
use move_core_types::account_address::AccountAddress;
use move_package::lock_file::schema::ManagedPackage;
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
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::io::Cursor;
use std::sync::{Arc, OnceLock};
use sui_move_build::PUBLISHED_AT_MANIFEST_FIELD;
use sui_sdk::wallet_context::WalletContext;
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::{
    BRIDGE_PACKAGE_ID, DEEPBOOK_PACKAGE_ID, MOVE_STDLIB_PACKAGE_ID, SUI_FRAMEWORK_PACKAGE_ID,
    SUI_SYSTEM_PACKAGE_ID,
};
use tokio::runtime::{Handle, Runtime};
use tokio::sync::Mutex;

use crate::package_cache::PackageCache;

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

struct Dependency {
    original_id: ObjectID,
    published_at: ObjectID,
}

/// CAUTION: This implementation has potential risks and limitations (see `run_async` for details):
///
/// 1. Using Handle::try_current() can lead to unexpected behavior:
///    - The detected runtime might be shutting down or specially configured.
///    - It may not be suitable for the intended task.
///    - Using an existing runtime can cause unintended interference with other parts of the application.
///
/// 2. block_in_place can cause issues in concurrent scenarios:
///    - It may block other tasks in the same runtime, potentially causing deadlocks.
///    - It can prevent progress in concurrent operations within the same task (e.g., in join! or select! macros).
///
/// Be aware of the potential for subtle bugs in complex async scenarios.
pub struct SuiPackageHooks {
    chain_id: Option<String>,
    package_cache: Arc<Mutex<PackageCache>>,
    handle: Handle,
}

impl SuiPackageHooks {
    pub fn new(chain_id: Option<String>, package_cache: PackageCache) -> Self {
        RUNTIME.get_or_init(|| Runtime::new().expect("Failed to create Tokio runtime"));

        Self {
            chain_id,
            package_cache: Arc::new(Mutex::new(package_cache)),
            handle: RUNTIME.get().unwrap().handle().clone(),
        }
    }

    pub async fn register_from_ctx(ctx: &WalletContext) -> Result<()> {
        let client = ctx.get_client().await?;
        let chain_id = client.read_api().get_chain_identifier().await.ok();
        let package_cache = PackageCache::new(client);
        move_package::package_hooks::register_package_hooks(Box::new(SuiPackageHooks::new(
            chain_id,
            package_cache,
        )));

        Ok(())
    }

    async fn resolve_original_id_chain(&self, id: ObjectID) -> anyhow::Result<AccountAddress> {
        let pkg = self.package_cache.lock().await.get(id).await?;
        let module =
            CompiledModule::deserialize_with_defaults(pkg.module_map.first_key_value().unwrap().1)
                .with_context(|| format!("Failed to resolve original-id for: {}", id))?;
        Ok(*module.address())
    }

    async fn resolve_version_chain(&self, id: ObjectID) -> anyhow::Result<SequenceNumber> {
        let pkg = self.package_cache.lock().await.get(id).await?;
        Ok(pkg.version)
    }

    fn resolve_original_id_and_version_lock(&self, lockfile: &str) -> Option<(Symbol, Symbol)> {
        let mut lockfile = Cursor::new(lockfile);
        let managed_packages = ManagedPackage::read(&mut lockfile).ok();

        managed_packages.and_then(|m| {
            let chain_id = self.chain_id.as_ref()?;
            m.into_iter()
                .find(|(_, v)| &v.chain_id == chain_id)
                .map(|(_, v)| {
                    (
                        Symbol::from(v.original_published_id),
                        Symbol::from(v.version),
                    )
                })
        })
    }

    pub fn run_async<F: std::future::Future>(&self, future: F) -> F::Output {
        if Handle::try_current().is_ok() {
            // Inside an existing Tokio runtime
            tokio::task::block_in_place(|| self.handle.block_on(future))
        } else {
            // Outside a Tokio runtime
            self.handle.block_on(future)
        }
    }
}

impl PackageHooks for SuiPackageHooks {
    fn custom_package_info_fields(&self) -> Vec<String> {
        vec![
            PUBLISHED_AT_MANIFEST_FIELD.to_string(),
            // TODO: remove this once version fields are removed from all manifests
            "version".to_string(),
        ]
    }

    fn resolve_on_chain_dependency(
        &self,
        dep_name: move_symbol_pool::Symbol,
        info: &OnChainInfo,
    ) -> Result<()> {
        let out_path = local_path(&DependencyKind::OnChain(info.clone()));
        if out_path.exists() {
            fs::remove_dir_all(&out_path)?;
        }

        let pkg_id = ObjectID::from_hex_literal(&info.id)
            .with_context(|| format!("Parsing dependency ID: {}", info.id))?;

        let pkg = self.run_async(async { self.package_cache.lock().await.get(pkg_id).await })?;

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

        // add address mappings for known packages
        const KNOWN_PACKAGES: [(ObjectID, &str); 5] = [
            (MOVE_STDLIB_PACKAGE_ID, "std"),
            (SUI_FRAMEWORK_PACKAGE_ID, "sui"),
            (SUI_SYSTEM_PACKAGE_ID, "sui_system"),
            (BRIDGE_PACKAGE_ID, "bridge"),
            (DEEPBOOK_PACKAGE_ID, "deepbook"),
        ];
        if let Some(&(id, name)) = KNOWN_PACKAGES.iter().find(|&&(id, _)| id == pkg_id) {
            writeln!(manifest, "\n[addresses]")?;
            writeln!(manifest, "{name} = \"{id}\"")?;
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

        self.run_async(async {
            self.package_cache
                .lock()
                .await
                .get_multi(all_deps_upgraded)
                .await
        })?;

        Ok(())
    }

    fn custom_resolve_pkg_id(
        &self,
        manifest: &SourceManifest,
        lockfile: Option<&str>,
    ) -> Result<Symbol> {
        if (!cfg!(debug_assertions) || cfg!(test))
            && manifest.package.edition == Some(Edition::DEVELOPMENT)
        {
            return Err(Edition::DEVELOPMENT.unknown_edition_error());
        }

        // If the lockfile is available and contains the relevant `env` entry we return that.
        if let Some(lockfile) = lockfile {
            if let Some((original_id, _)) = self.resolve_original_id_and_version_lock(lockfile) {
                return Ok(original_id);
            }
        }

        // Otherwise we try to resolve from the published-at field in the manifest.
        // If that is not available we assume that the package is not published and return the package name.
        let published_at = manifest
            .package
            .custom_properties
            .get(&Symbol::from("published-at"));
        match published_at {
            Some(published_at) => {
                let original_id = self.run_async(async {
                    self.resolve_original_id_chain(
                        ObjectID::from_hex_literal(published_at.as_str()).with_context(|| {
                            format!(
                                "Parsing published-at field value: {}",
                                published_at.as_str()
                            )
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
        lockfile: Option<&str>,
    ) -> Result<Option<Symbol>> {
        let published_at = manifest
            .package
            .custom_properties
            .get(&Symbol::from("published-at"));

        // If the lockfile is available and contains the relevant `env` entry we return that.
        if let Some(lockfile) = lockfile {
            if let Some((_, version)) = self.resolve_original_id_and_version_lock(lockfile) {
                return Ok(Some(version));
            }
        }

        // Otherwise we try to resolve from the published-at field in the manifest.
        // If that is not available we assume that the package is not published and return `None`.
        match published_at {
            Some(published_at) => {
                let version = self.run_async(async {
                    self.resolve_version_chain(
                        ObjectID::from_hex_literal(published_at.as_str()).with_context(|| {
                            format!(
                                "Parsing published-at field value: {}",
                                published_at.as_str()
                            )
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
